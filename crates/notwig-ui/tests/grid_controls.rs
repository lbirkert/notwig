use egui::{Pos2, Rect, Vec2};
use notwig_core::TimeMus;
use notwig_ui::SnapSettings;
use notwig_ui::test_api::timeline::snap_controls::{CONTROLS_WIDTH, SnapControlsRects, show};

fn frame(
    ctx: &egui::Context,
    snap: &mut SnapSettings,
    rect: Rect,
    events: Vec<egui::Event>,
    rects: &mut Option<SnapControlsRects>,
) {
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(640.0, 400.0))),
            events,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                *rects = Some(show(ui, rect, "test", snap, 64.0, TimeMus::new(4, 1)));
            });
        },
    );
}

fn button(pos: Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::default(),
    }
}

fn click(
    ctx: &egui::Context,
    snap: &mut SnapSettings,
    rect: Rect,
    pos: Pos2,
    rects: &mut Option<SnapControlsRects>,
) {
    frame(ctx, snap, rect, vec![egui::Event::PointerMoved(pos)], rects);
    frame(ctx, snap, rect, vec![button(pos, true)], rects);
    frame(ctx, snap, rect, vec![button(pos, false)], rects);
}

#[test]
fn snap_button_toggles_snapping() {
    let ctx = egui::Context::default();
    let rect = Rect::from_min_size(Pos2::new(300.0, 300.0), Vec2::new(200.0, 30.0));
    let mut snap = SnapSettings::default();
    let mut rects = None;
    frame(&ctx, &mut snap, rect, Vec::new(), &mut rects);
    let controls = rects.expect("controls rendered");
    assert!((controls.grid.right() - rect.right()).abs() < 0.5);
    assert!((controls.snap.width() + controls.grid.width()) < CONTROLS_WIDTH + 1.0);

    click(&ctx, &mut snap, rect, controls.snap.center(), &mut rects);
    assert!(!snap.enabled, "snap toggle should disable snapping");
    click(&ctx, &mut snap, rect, controls.snap.center(), &mut rects);
    assert!(snap.enabled, "snap toggle should re-enable snapping");
}

#[test]
fn grid_button_opens_settings_without_changing_snap() {
    let ctx = egui::Context::default();
    let rect = Rect::from_min_size(Pos2::new(300.0, 300.0), Vec2::new(200.0, 30.0));
    let mut snap = SnapSettings::default();
    let mut rects = None;
    frame(&ctx, &mut snap, rect, Vec::new(), &mut rects);
    let controls = rects.expect("controls rendered");
    let before = snap;
    click(&ctx, &mut snap, rect, controls.grid.center(), &mut rects);
    frame(&ctx, &mut snap, rect, Vec::new(), &mut rects);
    assert_eq!(snap, before);
}
