use std::collections::BTreeMap;
use std::sync::mpsc::Sender;

use js_sys::{Array, Reflect, Uint8Array};
use notwig_project::{BundleFiles, Manifest};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    File, FileSystemDirectoryHandle, FileSystemFileHandle, FileSystemGetDirectoryOptions,
    FileSystemGetFileOptions, FileSystemRemoveOptions, FileSystemWritableFileStream,
};

const PROJECTS_DIR: &str = "Projects";
const RECENT_PROJECTS_KEY: &str = "notwig.recent-projects";
const MAX_RECENT_PROJECTS: usize = 10;

#[derive(Debug)]
pub(crate) enum StorageEvent {
    ProjectLoaded {
        handle: ProjectHandle,
        files: BundleFiles,
    },
    ProjectSaved {
        handle: ProjectHandle,
    },
    ProjectsListed {
        epoch: u64,
        result: Result<Vec<String>, String>,
    },
    ProjectDownloaded {
        name: String,
        result: Result<(), String>,
    },
    AudioPicked {
        name: String,
        bytes: Vec<u8>,
    },
    ExtensionZipPicked {
        name: String,
        bytes: Vec<u8>,
    },
    FileSelectionCancelled,
    Error(String),
}

#[derive(Clone, Debug)]
pub(crate) struct ProjectHandle {
    pub(crate) dir: FileSystemDirectoryHandle,
    pub(crate) name: String,
}

pub(crate) async fn list_projects() -> Result<Vec<String>, String> {
    let root = projects_root().await?;
    let iterator = root.entries();
    let mut names = Vec::new();
    loop {
        let next = JsFuture::from(iterator.next().map_err(js_error)?)
            .await
            .map_err(js_error)?;
        if reflect_bool(&next, "done")?.unwrap_or(true) {
            break;
        }
        let value = Reflect::get(&next, &JsValue::from_str("value")).map_err(js_error)?;
        let pair = Array::from(&value);
        let Some(name) = pair.get(0).as_string() else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        if pair.get(1).dyn_into::<FileSystemDirectoryHandle>().is_ok() {
            names.push(name);
        }
    }
    names.sort();
    Ok(names)
}

pub(crate) async fn open_project(name: &str) -> Result<ProjectHandle, String> {
    project_handle(name, false).await
}

pub(crate) async fn create_project(name: &str) -> Result<ProjectHandle, String> {
    project_handle(name, true).await
}

async fn project_handle(name: &str, create: bool) -> Result<ProjectHandle, String> {
    let root = projects_root().await?;
    let value = if create {
        let options = FileSystemGetDirectoryOptions::new();
        options.set_create(true);
        JsFuture::from(root.get_directory_handle_with_options(name, &options))
            .await
            .map_err(js_error)?
    } else {
        JsFuture::from(root.get_directory_handle(name))
            .await
            .map_err(js_error)?
    };
    let dir: FileSystemDirectoryHandle = value.unchecked_into();
    Ok(ProjectHandle {
        dir,
        name: name.to_owned(),
    })
}

pub(crate) async fn delete_project(name: &str) -> Result<(), String> {
    let root = projects_root().await?;
    let options = FileSystemRemoveOptions::new();
    options.set_recursive(true);
    match JsFuture::from(root.remove_entry_with_options(name, &options)).await {
        Ok(_) => Ok(()),
        Err(error) if js_error_name(&error) == "NotFoundError" => Ok(()),
        Err(error) => Err(js_error(error)),
    }
}

pub(crate) async fn read_bundle(handle: &ProjectHandle) -> Result<BundleFiles, String> {
    let project = read_file(&handle.dir, "project.json").await?;
    let manifest: Manifest =
        serde_json::from_slice(&project).map_err(|error| format!("parse project.json: {error}"))?;
    let mut files = BundleFiles::default();
    files.insert("project.json", project);
    for asset in manifest.assets {
        if let Ok(bytes) = read_file(&handle.dir, &asset.uri).await {
            files.insert(asset.uri, bytes);
        }
    }
    Ok(files)
}

pub(crate) async fn read_project_files(handle: &ProjectHandle) -> Result<BundleFiles, String> {
    let files = read_directory(&handle.dir).await?;
    Ok(BundleFiles { files })
}

pub(crate) async fn write_bundle(
    handle: &ProjectHandle,
    files: &BundleFiles,
) -> Result<(), String> {
    for (path, bytes) in &files.files {
        write_file(&handle.dir, path, bytes).await?;
    }
    Ok(())
}

async fn projects_root() -> Result<FileSystemDirectoryHandle, String> {
    let root = opfs_root().await?;
    let options = FileSystemGetDirectoryOptions::new();
    options.set_create(true);
    let value = JsFuture::from(root.get_directory_handle_with_options(PROJECTS_DIR, &options))
        .await
        .map_err(js_error)?;
    value
        .dyn_into::<FileSystemDirectoryHandle>()
        .map_err(|_| "could not open the Projects folder".to_string())
}

async fn opfs_root() -> Result<FileSystemDirectoryHandle, String> {
    let window = web_sys::window().ok_or_else(|| "no browser window".to_string())?;
    let storage = window.navigator().storage();
    let value = JsFuture::from(storage.get_directory())
        .await
        .map_err(js_error)?;
    value
        .dyn_into::<FileSystemDirectoryHandle>()
        .map_err(|_| "could not open browser storage".to_string())
}

async fn read_directory(
    root: &FileSystemDirectoryHandle,
) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let mut files = BTreeMap::new();
    let mut pending = vec![(root.clone(), String::new())];
    while let Some((dir, prefix)) = pending.pop() {
        let iterator = dir.entries();
        loop {
            let next = JsFuture::from(iterator.next().map_err(js_error)?)
                .await
                .map_err(js_error)?;
            if reflect_bool(&next, "done")?.unwrap_or(true) {
                break;
            }
            let value = Reflect::get(&next, &JsValue::from_str("value")).map_err(js_error)?;
            let pair = Array::from(&value);
            let Some(name) = pair.get(0).as_string() else {
                continue;
            };
            let path = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            let handle = pair.get(1);
            if let Ok(child) = handle.clone().dyn_into::<FileSystemDirectoryHandle>() {
                pending.push((child, path));
            } else if let Ok(file_handle) = handle.dyn_into::<FileSystemFileHandle>() {
                let file: File = JsFuture::from(file_handle.get_file())
                    .await
                    .map_err(js_error)?
                    .unchecked_into();
                let buffer = JsFuture::from(file.array_buffer())
                    .await
                    .map_err(js_error)?;
                files.insert(path, Uint8Array::new(&buffer).to_vec());
            }
        }
    }
    Ok(files)
}

async fn read_file(root: &FileSystemDirectoryHandle, path: &str) -> Result<Vec<u8>, String> {
    let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    let Some((file_name, dirs)) = parts.split_last() else {
        return Err("empty file path".into());
    };
    let mut dir = root.clone();
    for part in dirs {
        let value = JsFuture::from(dir.get_directory_handle(part))
            .await
            .map_err(js_error)?;
        dir = value.unchecked_into();
    }
    let value = JsFuture::from(dir.get_file_handle(file_name))
        .await
        .map_err(js_error)?;
    let handle: FileSystemFileHandle = value.unchecked_into();
    let file: File = JsFuture::from(handle.get_file())
        .await
        .map_err(js_error)?
        .unchecked_into();
    let buffer = JsFuture::from(file.array_buffer())
        .await
        .map_err(js_error)?;
    Ok(Uint8Array::new(&buffer).to_vec())
}

async fn write_file(
    root: &FileSystemDirectoryHandle,
    path: &str,
    bytes: &[u8],
) -> Result<(), String> {
    let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    let Some((file_name, dirs)) = parts.split_last() else {
        return Err("empty file path".into());
    };
    let mut dir = root.clone();
    for part in dirs {
        let options = FileSystemGetDirectoryOptions::new();
        options.set_create(true);
        let value = JsFuture::from(dir.get_directory_handle_with_options(part, &options))
            .await
            .map_err(js_error)?;
        dir = value.unchecked_into();
    }
    let options = FileSystemGetFileOptions::new();
    options.set_create(true);
    let value = JsFuture::from(dir.get_file_handle_with_options(file_name, &options))
        .await
        .map_err(js_error)?;
    let handle: FileSystemFileHandle = value.unchecked_into();
    let value = JsFuture::from(handle.create_writable())
        .await
        .map_err(js_error)?;
    let stream: FileSystemWritableFileStream = value.unchecked_into();
    JsFuture::from(stream.write_with_u8_array(bytes).map_err(js_error)?)
        .await
        .map_err(js_error)?;
    JsFuture::from(stream.close()).await.map_err(js_error)?;
    Ok(())
}

fn reflect_bool(value: &JsValue, key: &str) -> Result<Option<bool>, String> {
    Ok(Reflect::get(value, &JsValue::from_str(key))
        .map_err(js_error)?
        .as_bool())
}

fn js_error_name(value: &JsValue) -> String {
    Reflect::get(value, &JsValue::from_str("name"))
        .ok()
        .and_then(|name| name.as_string())
        .unwrap_or_default()
}

fn js_error(value: JsValue) -> String {
    if let Some(message) = value.as_string() {
        return message;
    }
    let name = js_error_name(&value);
    let message = Reflect::get(&value, &JsValue::from_str("message"))
        .ok()
        .and_then(|message| message.as_string())
        .unwrap_or_default();
    if message.is_empty() {
        format!("{value:?}")
    } else if name.is_empty() {
        message
    } else {
        format!("{name}: {message}")
    }
}

pub(crate) fn remember_recent_project(name: &str) {
    let mut projects = recent_project_names();
    projects.retain(|project| project != name);
    projects.insert(0, name.to_owned());
    projects.truncate(MAX_RECENT_PROJECTS);
    if let Some(storage) =
        web_sys::window().and_then(|window| window.local_storage().ok().flatten())
    {
        let text = serde_json::to_string(&projects).unwrap_or_default();
        let _ = storage.set_item(RECENT_PROJECTS_KEY, &text);
    }
}

pub(crate) fn recent_project_names() -> Vec<String> {
    web_sys::window()
        .and_then(|window| window.local_storage().ok().flatten())
        .and_then(|storage| storage.get_item(RECENT_PROJECTS_KEY).ok().flatten())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub(crate) fn download_bytes(name: &str, mime: &str, bytes: &[u8]) -> Result<(), String> {
    let array = Uint8Array::from(bytes);
    let options = web_sys::BlobPropertyBag::new();
    options.set_type(mime);
    // A bare `Uint8Array` is iterable, so `new Blob(bytes)` stringifies each
    // numeric byte. Blob parts must be a sequence of BufferSource values.
    let parts: JsValue = Array::of1(&array.into()).into();
    let blob = web_sys::Blob::new_with_buffer_source_sequence_and_options(&parts, &options)
        .map_err(js_error)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(js_error)?;
    let window = web_sys::window().ok_or_else(|| "no browser window".to_string())?;
    let document = window
        .document()
        .ok_or_else(|| "no browser document".to_string())?;
    let element = document.create_element("a").map_err(js_error)?;
    let anchor: web_sys::HtmlAnchorElement = element.clone().unchecked_into();
    anchor.set_href(&url);
    anchor.set_download(name);
    let body = document.body();
    if let Some(body) = &body {
        if let Err(error) = body.append_child(element.as_ref()) {
            let _ = web_sys::Url::revoke_object_url(&url);
            return Err(js_error(error));
        }
    }
    anchor.click();
    // Defer cleanup to a later task so the browser can start the download
    // before the object URL is revoked. `once_into_js` releases the closure
    // after the timeout fires instead of leaking it via `forget`.
    let revoke_url = url.clone();
    let cleanup_element = element.clone();
    let cleanup: js_sys::Function = Closure::once_into_js(move || {
        let _ = web_sys::Url::revoke_object_url(&revoke_url);
        if let Some(parent) = cleanup_element.parent_node() {
            let _ = parent.remove_child(cleanup_element.as_ref());
        }
    })
    .unchecked_into();
    if window
        .set_timeout_with_callback_and_timeout_and_arguments_0(&cleanup, 1_000)
        .is_err()
    {
        let _ = web_sys::Url::revoke_object_url(&url);
        if let Some(body) = &body {
            let _ = body.remove_child(element.as_ref());
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
pub(crate) enum FilePurpose {
    Audio,
    ExtensionZip,
}

pub(crate) fn fallback_file_input(
    sender: Sender<StorageEvent>,
    accept: &str,
    purpose: FilePurpose,
) -> Result<(), String> {
    let document = web_sys::window()
        .and_then(|window| window.document())
        .ok_or_else(|| "no browser document".to_string())?;
    let input = document
        .create_element("input")
        .map_err(js_error)?
        .dyn_into::<web_sys::HtmlInputElement>()
        .map_err(|_| "could not create file input".to_string())?;
    input.set_type("file");
    input.set_accept(accept);
    let input_for_closure = input.clone();
    let change_sender = sender.clone();
    let closure = Closure::<dyn FnMut(web_sys::Event)>::new(move |_event| {
        let Some(file) = input_for_closure.files().and_then(|files| files.get(0)) else {
            let _ = change_sender.send(StorageEvent::FileSelectionCancelled);
            return;
        };
        let sender = change_sender.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let name = file.name();
            match JsFuture::from(file.array_buffer()).await {
                Ok(buffer) => {
                    let bytes = Uint8Array::new(&buffer).to_vec();
                    let event = match purpose {
                        FilePurpose::Audio => StorageEvent::AudioPicked { name, bytes },
                        FilePurpose::ExtensionZip => {
                            StorageEvent::ExtensionZipPicked { name, bytes }
                        }
                    };
                    let _ = sender.send(event);
                }
                Err(error) => {
                    let _ = sender.send(StorageEvent::Error(js_error(error)));
                }
            }
        });
    });
    input
        .add_event_listener_with_callback("change", closure.as_ref().unchecked_ref())
        .map_err(js_error)?;
    closure.forget();

    let cancel_closure = Closure::<dyn FnMut(web_sys::Event)>::new(move |_event| {
        let _ = sender.send(StorageEvent::FileSelectionCancelled);
    });
    input
        .add_event_listener_with_callback("cancel", cancel_closure.as_ref().unchecked_ref())
        .map_err(js_error)?;
    cancel_closure.forget();
    input.click();
    Ok(())
}

pub(crate) async fn read_opfs_optional(path: &str) -> Result<Option<Vec<u8>>, String> {
    let root = opfs_root().await?;
    match read_file(&root, path).await {
        Ok(bytes) => Ok(Some(bytes)),
        Err(_) => Ok(None),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OpfsEntryKind {
    File,
    Directory,
}

pub(crate) async fn list_opfs_entries(path: &str) -> Result<Vec<(String, OpfsEntryKind)>, String> {
    let root = opfs_root().await?;
    let Some(dir) = directory_at(&root, path, false).await? else {
        return Ok(Vec::new());
    };
    let iterator = dir.entries();
    let mut entries = Vec::new();
    loop {
        let next = JsFuture::from(iterator.next().map_err(js_error)?)
            .await
            .map_err(js_error)?;
        if reflect_bool(&next, "done")?.unwrap_or(true) {
            break;
        }
        let value = Reflect::get(&next, &JsValue::from_str("value")).map_err(js_error)?;
        let pair = Array::from(&value);
        let Some(name) = pair.get(0).as_string() else {
            continue;
        };
        let handle = pair.get(1);
        let kind = if handle
            .clone()
            .dyn_into::<FileSystemDirectoryHandle>()
            .is_ok()
        {
            OpfsEntryKind::Directory
        } else if handle.dyn_into::<FileSystemFileHandle>().is_ok() {
            OpfsEntryKind::File
        } else {
            continue;
        };
        entries.push((name, kind));
    }
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(entries)
}

pub(crate) async fn read_opfs_directory_files(
    path: &str,
) -> Result<Option<BTreeMap<String, Vec<u8>>>, String> {
    let root = opfs_root().await?;
    let Some(dir) = directory_at(&root, path, false).await? else {
        return Ok(None);
    };
    read_directory(&dir).await.map(Some)
}

pub(crate) async fn write_opfs_files(
    path: &str,
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<(), String> {
    let root = opfs_root().await?;
    let Some(dir) = directory_at(&root, path, true).await? else {
        return Err("could not create the extension directory".into());
    };
    for (relative, bytes) in files {
        write_file(&dir, relative, bytes).await?;
    }
    Ok(())
}

pub(crate) async fn remove_opfs_path(path: &str) -> Result<(), String> {
    let root = opfs_root().await?;
    let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    let Some((name, dirs)) = parts.split_last() else {
        return Err("empty path".into());
    };
    let mut dir = root;
    for part in dirs {
        match JsFuture::from(dir.get_directory_handle(part)).await {
            Ok(value) => dir = value.unchecked_into(),
            Err(error) if js_error_name(&error) == "NotFoundError" => return Ok(()),
            Err(error) => return Err(js_error(error)),
        }
    }
    let options = FileSystemRemoveOptions::new();
    options.set_recursive(true);
    match JsFuture::from(dir.remove_entry_with_options(name, &options)).await {
        Ok(_) => Ok(()),
        Err(error) if js_error_name(&error) == "NotFoundError" => Ok(()),
        Err(error) => Err(js_error(error)),
    }
}

async fn directory_at(
    root: &FileSystemDirectoryHandle,
    path: &str,
    create: bool,
) -> Result<Option<FileSystemDirectoryHandle>, String> {
    let mut dir = root.clone();
    for part in path.split('/').filter(|part| !part.is_empty()) {
        let value = if create {
            let options = FileSystemGetDirectoryOptions::new();
            options.set_create(true);
            JsFuture::from(dir.get_directory_handle_with_options(part, &options)).await
        } else {
            JsFuture::from(dir.get_directory_handle(part)).await
        };
        match value {
            Ok(value) => dir = value.unchecked_into(),
            Err(error) if !create && js_error_name(&error) == "NotFoundError" => return Ok(None),
            Err(error) => return Err(js_error(error)),
        }
    }
    Ok(Some(dir))
}

pub(crate) async fn fetch_bytes(url: &str) -> Result<Option<Vec<u8>>, String> {
    let window = web_sys::window().ok_or_else(|| "no browser window".to_string())?;
    let value = JsFuture::from(window.fetch_with_str(url))
        .await
        .map_err(js_error)?;
    let response: web_sys::Response = value.unchecked_into();
    if !response.ok() {
        return Ok(None);
    }
    let buffer = JsFuture::from(response.array_buffer().map_err(js_error)?)
        .await
        .map_err(js_error)?;
    Ok(Some(Uint8Array::new(&buffer).to_vec()))
}
