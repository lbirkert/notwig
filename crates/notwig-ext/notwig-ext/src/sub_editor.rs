use std::collections::BTreeMap;
use std::sync::Arc;

use notwig_ext_api::{
    AudioSubEditorContext, AudioSubEditorFrame, AudioSubEditorHost, ExtensionCapability,
};

use crate::compiled::LoadedExtensions;
use crate::instance::ExtensionInstance;

struct UiInstance {
    instance: ExtensionInstance,
    handle: i32,
}

impl Drop for UiInstance {
    fn drop(&mut self) {
        self.instance.destroy_audio_sub_editor(self.handle);
    }
}

/// Host-side drawer for extension-rendered audio sub-editors.
///
/// Instances are keyed by clip and contribution id so switching away from a
/// custom editor and back preserves its view state.
pub struct ExtensionAudioSubEditorHost {
    loaded: Arc<LoadedExtensions>,
    instances: BTreeMap<String, UiInstance>,
}

impl ExtensionAudioSubEditorHost {
    pub(crate) fn new(loaded: Arc<LoadedExtensions>) -> Self {
        Self {
            loaded,
            instances: BTreeMap::new(),
        }
    }
}

impl AudioSubEditorHost for ExtensionAudioSubEditorHost {
    fn draw_audio_sub_editor(
        &mut self,
        editor_id: &str,
        context: &AudioSubEditorContext,
    ) -> Option<AudioSubEditorFrame> {
        let extension = self.loaded.find_audio_sub_editor(editor_id)?;
        if !extension
            .manifest
            .has_capability(ExtensionCapability::AudioEditor)
        {
            return None;
        }
        let renderer = extension
            .audio_sub_editors
            .iter()
            .find(|editor| editor.id == editor_id)
            .and_then(|editor| editor.custom())
            .map(|custom| custom.renderer.as_str())?;
        let runtime = extension.runtime.as_ref()?;
        let key = format!("{}:{}", context.clip_id, editor_id);
        if !self.instances.contains_key(&key) {
            let mut instance = ExtensionInstance::instantiate(
                runtime,
                &self.loaded.engines,
                &extension.manifest.id,
            )
            .map_err(|error| {
                tracing::warn!(
                    extension = %extension.manifest.id,
                    editor = editor_id,
                    error = %error,
                    "could not create extension audio sub-editor instance"
                );
                error
            })
            .ok()?;
            let handle = instance.create_audio_sub_editor(renderer).ok()?;
            self.instances
                .insert(key.clone(), UiInstance { instance, handle });
        }
        let result = {
            let ui = self.instances.get_mut(&key)?;
            ui.instance.draw_audio_sub_editor(ui.handle, context)
        };
        match result {
            Ok(frame) => Some(frame),
            Err(error) => {
                tracing::warn!(
                    extension = %extension.manifest.id,
                    editor = editor_id,
                    clip = %context.clip_id,
                    error = %error,
                    "extension audio sub-editor draw failed"
                );
                None
            }
        }
    }
}
