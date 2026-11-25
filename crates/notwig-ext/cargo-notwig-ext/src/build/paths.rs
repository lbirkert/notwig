//! Package path and file-name rules.

use std::path::{Component, Path};

use notwig_ext_api::manifest::ExtensionManifest;

use crate::error::{Error, Result};

pub(super) fn sanitized_id(manifest: &ExtensionManifest) -> String {
    manifest
        .id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect()
}

pub(super) fn ensure_zip_extension(file_name: String) -> Result<String> {
    let file_name = if Path::new(&file_name)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
    {
        file_name
    } else {
        format!("{file_name}.zip")
    };
    if Path::new(&file_name).components().count() != 1 {
        return Err(Error::message("--name must be a file name, not a path"));
    }
    Ok(file_name)
}

pub(super) fn safe_relative(value: &str) -> Result<String> {
    let path = Path::new(value);
    if value.trim().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(Error::message(format!(
            "package path must be relative and stay inside the extension: {value}"
        )));
    }
    Ok(path
        .components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/"))
}
