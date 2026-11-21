#[test]
fn embedded_brand_variants_decode_for_the_top_bar() {
    let ctx = egui::Context::default();
    let dark = notwig_ui::test_api::widgets::brand::icon_size(&ctx, true);
    let light = notwig_ui::test_api::widgets::brand::icon_size(&ctx, false);
    assert!(dark[0] > 0 && dark[1] > 0);
    assert_eq!(dark, light);
}
