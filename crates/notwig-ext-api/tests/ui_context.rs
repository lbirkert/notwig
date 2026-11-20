use notwig_core::{ParamId, ParamValue};
use notwig_ext_api::{
    PanelAction, PanelCommand, button,
    geometry::{Point, Rect, Size},
    knob,
    ui::{RecordUi, Sense, Style, UiContext, UiId, UiInput, UiMemory},
};

fn target() -> Rect {
    Rect::from_min_size(Point::new(0.0, 0.0), Size::new(300.0, 200.0))
}

fn frame(
    input: &UiInput,
    memory: &mut UiMemory,
    add: impl FnOnce(&mut UiContext<'_>),
) -> notwig_ext_api::PanelDraw {
    let mut record = RecordUi::new(input, memory);
    {
        let mut ui = UiContext::new(&mut record, Style::default(), target());
        add(&mut ui);
    }
    record.into_panel()
}

fn pos(x: f32, y: f32) -> UiInput {
    UiInput {
        pointer: notwig_ext_api::PointerState {
            pos: Some(Point::new(x, y)),
            ..Default::default()
        },
        time: 1.0,
        ..Default::default()
    }
}

#[test]
fn button_clicks_on_press_then_release() {
    let mut memory = UiMemory::new();
    let mut hover = pos(10.0, 10.0);
    let mut clicked = false;
    let panel = frame(&hover, &mut memory, |ui| {
        clicked = button(ui, "Test").clicked();
    });
    assert!(!clicked);
    assert_ne!(panel.commands.len(), 0);

    hover.pointer.primary_pressed = true;
    hover.pointer.primary_down = true;
    clicked = false;
    frame(&hover, &mut memory, |ui| {
        clicked = button(ui, "Test").clicked();
    });
    assert!(!clicked);

    hover.pointer.primary_pressed = false;
    hover.pointer.primary_down = false;
    hover.pointer.primary_released = true;
    clicked = false;
    frame(&hover, &mut memory, |ui| {
        clicked = button(ui, "Test").clicked();
    });
    assert!(clicked);
}

#[test]
fn click_outside_release_does_not_click() {
    let mut memory = UiMemory::new();
    let mut input = pos(10.0, 10.0);
    frame(&input, &mut memory, |ui| {
        button(ui, "Test");
    });

    input.pointer.primary_pressed = true;
    input.pointer.primary_down = true;
    frame(&input, &mut memory, |ui| {
        button(ui, "Test");
    });

    input.pointer.pos = Some(Point::new(200.0, 150.0));
    input.pointer.primary_pressed = false;
    input.pointer.primary_down = false;
    input.pointer.primary_released = true;
    let mut clicked = false;
    frame(&input, &mut memory, |ui| {
        clicked = button(ui, "Test").clicked();
    });
    assert!(!clicked);
}

#[test]
fn drag_changes_knob_and_reports_drag() {
    let mut memory = UiMemory::new();
    let mut input = pos(40.0, 40.0);
    let rect = Rect::from_min_size(Point::new(10.0, 10.0), Size::new(78.0, 84.0));
    let _ = frame(&input, &mut memory, |ui| {
        knob(ui, UiId::new(1), rect, 0.5, 0.5, false, "Gain");
    });

    input.pointer.primary_pressed = true;
    input.pointer.primary_down = true;
    let _ = frame(&input, &mut memory, |ui| {
        knob(ui, UiId::new(1), rect, 0.5, 0.5, false, "Gain");
    });

    input.pointer.primary_pressed = false;
    input.pointer.pos = Some(Point::new(40.0, 20.0));
    input.pointer.delta = Point::new(0.0, -20.0);
    let mut changed = None;
    let _ = frame(&input, &mut memory, |ui| {
        changed = knob(ui, UiId::new(1), rect, 0.5, 0.5, false, "Gain");
    });
    assert_eq!(changed, Some(0.6));
}

#[test]
fn actions_are_recorded_for_host_validation() {
    let mut memory = UiMemory::new();
    let mut input = pos(10.0, 10.0);
    frame(&input, &mut memory, |ui| {
        button(ui, "Reset");
    });
    input.pointer.primary_pressed = true;
    input.pointer.primary_down = true;
    frame(&input, &mut memory, |ui| {
        button(ui, "Reset");
    });
    input.pointer.primary_pressed = false;
    input.pointer.primary_down = false;
    input.pointer.primary_released = true;
    let panel = frame(&input, &mut memory, |ui| {
        if button(ui, "Reset").clicked() {
            ui.set_param_normalized(ParamId::nil(), 0.25);
            ui.set_param_value(ParamId::nil(), ParamValue::F64(1.0));
        }
    });
    assert!(panel.actions.contains(&PanelAction::SetParamNormalized {
        param_id: ParamId::nil(),
        norm: 0.25
    }));
    assert!(panel.actions.contains(&PanelAction::SetParamValue {
        param_id: ParamId::nil(),
        value: ParamValue::F64(1.0)
    }));
}

#[test]
fn clipped_widget_does_not_interact_outside_clip() {
    let mut memory = UiMemory::new();
    let input = pos(10.0, 80.0);
    let rect = Rect::from_min_size(Point::new(0.0, 0.0), Size::new(100.0, 100.0));
    let clip = Rect::from_min_size(Point::new(0.0, 0.0), Size::new(100.0, 50.0));
    let mut hovered = true;
    frame(&input, &mut memory, |ui| {
        ui.push_clip(clip);
        hovered = ui.interact(UiId::new(9), rect, Sense::click()).hovered();
        ui.pop_clip();
    });
    let mut hovered_second = true;
    frame(&input, &mut memory, |ui| {
        ui.push_clip(clip);
        hovered_second = ui.interact(UiId::new(9), rect, Sense::click()).hovered();
        ui.pop_clip();
    });
    assert!(!hovered);
    assert!(!hovered_second);
}

#[test]
fn commands_are_recorded_in_paint_order() {
    let input = UiInput::default();
    let mut memory = UiMemory::new();
    let panel = frame(&input, &mut memory, |ui| {
        ui.painter().fill_rect(
            Rect::from_min_size(Point::new(0.0, 0.0), Size::new(10.0, 10.0)),
            0.0,
            notwig_ext_api::Color::WHITE,
        );
        ui.painter().fill_rect(
            Rect::from_min_size(Point::new(10.0, 0.0), Size::new(10.0, 10.0)),
            0.0,
            notwig_ext_api::Color::BLACK,
        );
    });
    assert_eq!(panel.commands.len(), 2);
    assert!(matches!(panel.commands[0], PanelCommand::Draw(_)));
    assert!(matches!(panel.commands[1], PanelCommand::Draw(_)));
}

#[test]
fn stereo_meter_paints_inside_its_rect() {
    let input = UiInput::default();
    let mut memory = UiMemory::new();
    let panel = frame(&input, &mut memory, |ui| {
        notwig_ext_api::stereo_meter(ui, Size::new(20.0, 80.0), 1.0, 0.0, 0.5);
    });
    panel.validate().unwrap();
    assert!(panel.commands.len() > 4);
}

#[test]
fn horizontal_row_advances_the_vertical_cursor() {
    let input = UiInput::default();
    let mut memory = UiMemory::new();
    let mut below = None;
    frame(&input, &mut memory, |ui| {
        ui.horizontal(|ui| {
            ui.allocate(Size::new(20.0, 20.0));
            ui.allocate(Size::new(20.0, 20.0));
        });
        below = Some(ui.allocate(Size::new(10.0, 10.0)));
    });
    assert!(below.is_some_and(|rect| rect.min.y >= 20.0));
}
