use std::sync::mpsc::Sender;

use js_sys::{Array, Function, Promise, Reflect, Uint8Array};
use notwig_project::{BundleFiles, Manifest};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    File, FileSystemDirectoryHandle, FileSystemFileHandle, FileSystemGetDirectoryOptions,
    FileSystemGetFileOptions, FileSystemWritableFileStream,
};

#[derive(Debug)]
pub(crate) enum StorageEvent {
    ProjectLoaded {
        handle: ProjectHandle,
        files: BundleFiles,
    },
    ProjectSaved {
        handle: ProjectHandle,
    },
    AudioPicked {
        name: String,
        bytes: Vec<u8>,
    },
    ExtensionZipPicked {
        name: String,
        bytes: Vec<u8>,
    },
    Error(String),
}

#[derive(Clone, Debug)]
pub(crate) struct ProjectHandle {
    pub(crate) dir: FileSystemDirectoryHandle,
    pub(crate) name: String,
}

pub(crate) fn request_directory() -> Result<Promise, String> {
    let window = web_sys::window().ok_or_else(|| "no browser window".to_string())?;
    let function =
        Reflect::get(&window, &JsValue::from_str("showDirectoryPicker")).map_err(js_error)?;
    if !function.is_function() {
        return Err("this browser does not support the File System Access API".into());
    }
    let function: Function = function.unchecked_into();
    function
        .call0(&window)
        .map(wasm_bindgen::JsCast::unchecked_into)
        .map_err(js_error)
}

pub(crate) fn request_file() -> Result<Promise, String> {
    let window = web_sys::window().ok_or_else(|| "no browser window".to_string())?;
    let function =
        Reflect::get(&window, &JsValue::from_str("showOpenFilePicker")).map_err(js_error)?;
    if !function.is_function() {
        return Err("this browser does not support the File System Access API".into());
    }
    let function: Function = function.unchecked_into();
    let options = js_sys::Object::new();
    Reflect::set(&options, &JsValue::from_str("multiple"), &JsValue::FALSE).map_err(js_error)?;
    function
        .call1(&window, &options)
        .map(wasm_bindgen::JsCast::unchecked_into)
        .map_err(js_error)
}

pub(crate) async fn directory_from_promise(promise: Promise) -> Result<ProjectHandle, String> {
    let value = JsFuture::from(promise).await.map_err(js_error)?;
    let dir: FileSystemDirectoryHandle = value.unchecked_into();
    let name = dir.name();
    Ok(ProjectHandle { dir, name })
}

pub(crate) async fn first_file_from_promise(promise: Promise) -> Result<(String, Vec<u8>), String> {
    let value = JsFuture::from(promise).await.map_err(js_error)?;
    let files: Array = value.unchecked_into();
    let first = files
        .get(0)
        .dyn_into::<FileSystemFileHandle>()
        .map_err(|_| "no file was selected".to_string())?;
    let file: File = JsFuture::from(first.get_file())
        .await
        .map_err(js_error)?
        .unchecked_into();
    let name = file.name();
    let buffer = JsFuture::from(file.array_buffer())
        .await
        .map_err(js_error)?;
    let bytes = Uint8Array::new(&buffer).to_vec();
    Ok((name, bytes))
}

pub(crate) async fn open_opfs_project(name: &str) -> Result<ProjectHandle, String> {
    let root = opfs_root().await?;
    let options = FileSystemGetDirectoryOptions::new();
    options.set_create(true);
    let value = JsFuture::from(root.get_directory_handle_with_options(name, &options))
        .await
        .map_err(js_error)?;
    let dir: FileSystemDirectoryHandle = value.unchecked_into();
    Ok(ProjectHandle {
        dir,
        name: name.to_owned(),
    })
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

pub(crate) async fn write_bundle(
    handle: &ProjectHandle,
    files: &BundleFiles,
) -> Result<(), String> {
    for (path, bytes) in &files.files {
        write_file(&handle.dir, path, bytes).await?;
    }
    Ok(())
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

fn js_error(value: JsValue) -> String {
    value.as_string().unwrap_or_else(|| format!("{value:?}"))
}

const LAST_PROJECT_KEY: &str = "notwig.last-project";

pub(crate) fn remember_last_project(name: &str) {
    if let Some(storage) =
        web_sys::window().and_then(|window| window.local_storage().ok().flatten())
    {
        let _ = storage.set_item(LAST_PROJECT_KEY, name);
    }
}

pub(crate) fn last_project_name() -> Option<String> {
    web_sys::window()
        .and_then(|window| window.local_storage().ok().flatten())
        .and_then(|storage| storage.get_item(LAST_PROJECT_KEY).ok().flatten())
        .filter(|name| !name.is_empty())
}

pub(crate) fn download_bytes(name: &str, mime: &str, bytes: &[u8]) -> Result<(), String> {
    let array = Uint8Array::from(bytes);
    let options = web_sys::BlobPropertyBag::new();
    options.set_type(mime);
    let blob =
        web_sys::Blob::new_with_u8_array_sequence_and_options(&JsValue::from(array), &options)
            .map_err(js_error)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(js_error)?;
    let document = web_sys::window()
        .and_then(|window| window.document())
        .ok_or_else(|| "no browser document".to_string())?;
    let element = document.create_element("a").map_err(js_error)?;
    let anchor: web_sys::HtmlAnchorElement = element.clone().unchecked_into();
    anchor.set_href(&url);
    anchor.set_download(name);
    if let Some(body) = document.body() {
        let _ = body.append_child(element.as_ref());
        anchor.click();
        let _ = body.remove_child(element.as_ref());
    } else {
        anchor.click();
    }
    let _ = web_sys::Url::revoke_object_url(&url);
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
    let closure = Closure::<dyn FnMut(web_sys::Event)>::new(move |_event| {
        let Some(files) = input_for_closure.files() else {
            return;
        };
        let Some(file) = files.get(0) else {
            return;
        };
        let sender = sender.clone();
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

pub(crate) async fn write_opfs(path: &str, bytes: &[u8]) -> Result<(), String> {
    let root = opfs_root().await?;
    write_file(&root, path, bytes).await
}

pub(crate) async fn remove_opfs_file(path: &str) -> Result<(), String> {
    let root = opfs_root().await?;
    let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    let Some((file_name, dirs)) = parts.split_last() else {
        return Err("empty file path".into());
    };
    let mut dir = root;
    for part in dirs {
        let value = JsFuture::from(dir.get_directory_handle(part))
            .await
            .map_err(js_error)?;
        dir = value.unchecked_into();
    }
    JsFuture::from(dir.remove_entry(file_name))
        .await
        .map_err(js_error)?;
    Ok(())
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
