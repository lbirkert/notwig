use std::collections::BTreeMap;
use std::sync::Arc;

use notwig_ext_api::{
    AudioSubEditorContext, AudioSubEditorFrame, AudioSubEditorHost, ExtensionCapability,
};

use crate::web::compiled::WebLoadedExtensions;
use crate::web::instance::WebExtensionInstance;

struct UiInstance {
    instance: WebExtensionInstance,
    handle: i32,
}

impl Drop for UiInstance {
    fn drop(&mut self) {
        self.instance.destroy_audio_sub_editor(self.handle);
    }
}

pub(crate) struct WebExtensionAudioSubEditorHost {
    loaded: Arc<WebLoadedExtensions>,
    instances: BTreeMap<String, UiInstance>,
}

impl WebExtensionAudioSubEditorHost {
    pub(crate) fn new(loaded: Arc<WebLoadedExtensions>) -> Self {
        Self {
            loaded,
            instances: BTreeMap::new(),
        }
    }
}

impl AudioSubEditorHost for WebExtensionAudioSubEditorHost {
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
        extension
            .audio_sub_editors
            .iter()
            .find(|editor| editor.id == editor_id)?;
        let key = format!("{}:{}", context.clip_id, editor_id);
        if !self.instances.contains_key(&key) {
            let mut instance =
                WebExtensionInstance::instantiate(&extension.module, &extension.manifest.id)
                    .map_err(|error| {
                        tracing::warn!(
                            extension = %extension.manifest.id,
                            editor = editor_id,
                            error = %error,
                            "could not create browser extension audio sub-editor instance"
                        );
                        error
                    })
                    .ok()?;
            let handle = instance.create_audio_sub_editor(editor_id).ok()?;
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
                    "browser extension audio sub-editor draw failed"
                );
                None
            }
        }
    }
}
