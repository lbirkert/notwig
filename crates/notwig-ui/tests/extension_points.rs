use notwig_ext_api::{
    BottomPanelTarget, CORE_EXTENSION_ID, CORE_EXTENSION_NAME, EP_NOTWIG_BOTTOM_PANEL,
};

#[test]
fn core_bottom_panels_share_one_point_and_use_notwig_core_owner() {
    let descriptors = notwig_ui::test_api::panel_registry::core_bottom_panel_descriptors();
    assert_eq!(descriptors.len(), 3);
    assert_eq!(CORE_EXTENSION_NAME, "Notwig Core");

    let mut targets = Vec::new();
    for (point, owner, id, name, target) in descriptors {
        assert_eq!(point, EP_NOTWIG_BOTTOM_PANEL.id());
        assert_eq!(owner, CORE_EXTENSION_ID);
        assert!(id.starts_with("notwig.core."));
        assert_ne!(name, "");
        targets.push(target);
    }
    assert!(targets.contains(&BottomPanelTarget::DeviceEditor));
    assert!(targets.contains(&BottomPanelTarget::MidiEditor));
    assert!(targets.contains(&BottomPanelTarget::AudioEditor));
}
