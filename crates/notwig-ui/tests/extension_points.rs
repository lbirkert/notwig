use notwig_ext_api::{
    CORE_EXTENSION_ID, CORE_EXTENSION_NAME, EP_NOTWIG_AUDIO_EDITOR_BOTTOM_PANEL,
    EP_NOTWIG_DEVICE_EDITOR_BOTTOM_PANEL, EP_NOTWIG_MIDI_EDITOR_BOTTOM_PANEL,
};

#[test]
fn core_bottom_panels_use_extension_points_and_notwig_core_owner() {
    let descriptors = notwig_ui::test_api::panel_registry::core_bottom_panel_descriptors();
    assert_eq!(descriptors.len(), 3);
    assert_eq!(CORE_EXTENSION_NAME, "Notwig Core");
    for (point, owner, name) in descriptors {
        assert_eq!(owner, CORE_EXTENSION_ID);
        assert_ne!(name, "");
        assert!(
            [
                EP_NOTWIG_DEVICE_EDITOR_BOTTOM_PANEL.id(),
                EP_NOTWIG_MIDI_EDITOR_BOTTOM_PANEL.id(),
                EP_NOTWIG_AUDIO_EDITOR_BOTTOM_PANEL.id(),
            ]
            .contains(&point)
        );
    }
}
