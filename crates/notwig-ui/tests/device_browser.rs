use notwig_ui::test_api::device_editor::plugin_format_tag;

#[test]
fn plugin_entries_show_a_format_tag() {
    assert_eq!(plugin_format_tag("vst3.ABC"), Some("VST3"));
    assert_eq!(plugin_format_tag("vst2.1234"), Some("VST"));
    assert_eq!(plugin_format_tag("au.aufx:ABCD:Appl"), Some("AU"));
    assert_eq!(plugin_format_tag("au3.aufx:ABCD:Appl"), Some("AU3"));
    assert_eq!(plugin_format_tag("clap.com.example.synth"), Some("CLAP"));
    assert_eq!(
        plugin_format_tag("lv2.http://example.org/plug"),
        Some("LV2")
    );
    assert_eq!(plugin_format_tag("dev.notwig.eq"), None);
    assert_eq!(plugin_format_tag("notwig.tool"), None);
}
