use notwig_plugins::{PluginDescriptor, PluginFormat};

#[test]
fn format_prefixes_are_stable() {
    assert_eq!(PluginFormat::Vst3.prefix(), "vst3");
    assert_eq!(PluginFormat::Au.prefix(), "au");
    assert_eq!(PluginFormat::Vst2.prefix(), "vst2");
    assert_eq!(PluginFormat::Clap.prefix(), "clap");
    assert_eq!(PluginFormat::Lv2.prefix(), "lv2");
    assert_eq!(PluginFormat::Au3.prefix(), "au3");
}

#[test]
fn descriptor_type_id_uses_the_format_prefix() {
    assert_eq!(
        PluginDescriptor::type_id_for(PluginFormat::Vst3, "abc"),
        "vst3.abc"
    );
    assert_eq!(
        PluginDescriptor::type_id_for(PluginFormat::Au, "abc"),
        "au.abc"
    );
    assert_eq!(
        PluginDescriptor::type_id_for(PluginFormat::Vst2, "abc"),
        "vst2.abc"
    );
    assert_eq!(
        PluginDescriptor::type_id_for(PluginFormat::Clap, "abc"),
        "clap.abc"
    );
    assert_eq!(
        PluginDescriptor::type_id_for(PluginFormat::Lv2, "abc"),
        "lv2.abc"
    );
    assert_eq!(
        PluginDescriptor::type_id_for(PluginFormat::Au3, "abc"),
        "au3.abc"
    );
}
