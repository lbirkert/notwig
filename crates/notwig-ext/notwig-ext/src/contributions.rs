//! Shared runtime contribution resolution for native and browser hosts.

use std::collections::BTreeMap;

use notwig_ext_api::{
    AudioSubEditorDescriptor, CommandDescriptor, ExtensionManifest, ExtensionPointDescriptor,
    ExtensionRegistrations, MAX_COMMAND_DESCRIPTION_BYTES, REGISTRATION_MANIFEST_VERSION,
    validate_when,
};

use crate::error::ExtensionError;

pub(crate) struct ResolvedContributions {
    pub(crate) points: Vec<ExtensionPointDescriptor>,
    pub(crate) commands: Vec<CommandDescriptor>,
    pub(crate) audio_sub_editors: Vec<AudioSubEditorDescriptor>,
}

pub(crate) fn resolve(
    manifest: &ExtensionManifest,
    runtime: Option<ExtensionRegistrations>,
) -> Result<ResolvedContributions, ExtensionError> {
    let runtime = runtime.unwrap_or_default();
    if runtime.version != REGISTRATION_MANIFEST_VERSION {
        return Err(ExtensionError::Abi(format!(
            "unsupported registration manifest version {}",
            runtime.version
        )));
    }

    let mut commands: BTreeMap<String, CommandDescriptor> = manifest
        .command_contributions()
        .map(|contribution| {
            (
                contribution.id.clone(),
                CommandDescriptor {
                    id: contribution.id.clone(),
                    title: contribution.title.clone().unwrap_or_default(),
                    category: contribution.category.clone().unwrap_or_default(),
                    description: contribution.description.clone().unwrap_or_default(),
                    when: contribution.when.clone(),
                    default_bindings: contribution.default_bindings.clone(),
                    extension_id: Some(manifest.id.clone()),
                },
            )
        })
        .collect();

    for mut command in runtime.commands {
        command.extension_id = Some(manifest.id.clone());
        if let Some(manifest_command) = commands.get(&command.id) {
            merge_command(&mut command, manifest_command);
        }
        validate_command(&command)?;
        commands.insert(command.id.clone(), command);
    }

    let mut audio_sub_editors: BTreeMap<String, AudioSubEditorDescriptor> = manifest
        .audio_sub_editor_contributions()
        .map(|contribution| {
            (
                contribution.id.clone(),
                AudioSubEditorDescriptor {
                    id: contribution.id.clone(),
                    title: contribution.title.clone().unwrap_or_default(),
                    category: contribution.category.clone().unwrap_or_default(),
                    order: contribution.order,
                    extension_id: Some(manifest.id.clone()),
                },
            )
        })
        .collect();

    for mut editor in runtime.audio_sub_editors {
        editor.extension_id = Some(manifest.id.clone());
        if let Some(manifest_editor) = audio_sub_editors.get(&editor.id) {
            merge_audio_sub_editor(&mut editor, manifest_editor);
        }
        audio_sub_editors.insert(editor.id.clone(), editor);
    }

    Ok(ResolvedContributions {
        points: runtime.points,
        commands: commands.into_values().collect(),
        audio_sub_editors: audio_sub_editors.into_values().collect(),
    })
}

fn merge_command(runtime: &mut CommandDescriptor, manifest: &CommandDescriptor) {
    if runtime.title.is_empty() {
        runtime.title.clone_from(&manifest.title);
    }
    if runtime.category.is_empty() {
        runtime.category.clone_from(&manifest.category);
    }
    if runtime.description.is_empty() {
        runtime.description.clone_from(&manifest.description);
    }
    if runtime.when.is_none() {
        runtime.when.clone_from(&manifest.when);
    }
    if runtime.default_bindings.is_empty() {
        runtime
            .default_bindings
            .clone_from(&manifest.default_bindings);
    }
}

fn merge_audio_sub_editor(
    runtime: &mut AudioSubEditorDescriptor,
    manifest: &AudioSubEditorDescriptor,
) {
    if runtime.title.is_empty() {
        runtime.title.clone_from(&manifest.title);
    }
    if runtime.category.is_empty() {
        runtime.category.clone_from(&manifest.category);
    }
    if runtime.order == 0 {
        runtime.order = manifest.order;
    }
}

fn validate_command(command: &CommandDescriptor) -> Result<(), ExtensionError> {
    if command.title.trim().is_empty() || command.category.trim().is_empty() {
        return Err(ExtensionError::Abi(format!(
            "command {} must declare a title and category",
            command.id
        )));
    }
    if command.description.len() > MAX_COMMAND_DESCRIPTION_BYTES {
        return Err(ExtensionError::Abi(format!(
            "command {} description is too long",
            command.id
        )));
    }
    if let Some(when) = &command.when {
        validate_when(when).map_err(|error| {
            ExtensionError::Abi(format!("command {} has invalid when: {error}", command.id))
        })?;
    }
    Ok(())
}
