use notwig_ext_api::{
    Color, DrawCommand, DrawError, DrawList, MAX_COMMANDS, MAX_PATH_POINTS, Point, Rect, Size,
    Stroke,
};

fn rect() -> Rect {
    Rect::from_min_size(Point::new(0.0, 0.0), Size::new(10.0, 10.0))
}

#[test]
fn valid_draw_list_passes() {
    let list = DrawList {
        commands: vec![
            DrawCommand::FillRect {
                rect: rect(),
                radius: 2.0,
                color: Color::WHITE,
            },
            DrawCommand::Line {
                from: Point::new(0.0, 0.0),
                to: Point::new(1.0, 1.0),
                width: 1.0,
                color: Color::WHITE,
            },
            DrawCommand::Text {
                pos: Point::new(0.0, 0.0),
                text: "hello".into(),
                size: 10.0,
                color: Color::WHITE,
                align: notwig_ext_api::TextAlign::Left,
            },
        ],
    };
    assert_eq!(list.validate(), Ok(()));
}

#[test]
fn non_finite_geometry_is_rejected() {
    let list = DrawList {
        commands: vec![DrawCommand::CircleFill {
            center: Point::new(f32::NAN, 0.0),
            radius: 1.0,
            color: Color::WHITE,
        }],
    };
    assert_eq!(list.validate(), Err(DrawError::NonFinite { index: 0 }));
}

#[test]
fn inverted_rect_is_rejected() {
    let list = DrawList {
        commands: vec![DrawCommand::FillRect {
            rect: Rect::new(Point::new(3.0, 3.0), Point::new(1.0, 1.0)),
            radius: 0.0,
            color: Color::WHITE,
        }],
    };
    assert_eq!(list.validate(), Err(DrawError::InvalidRect { index: 0 }));
}

#[test]
fn excessive_path_points_are_rejected() {
    let points = vec![Point::new(0.0, 0.0); MAX_PATH_POINTS + 1];
    let list = DrawList {
        commands: vec![DrawCommand::Path {
            points,
            closed: false,
            fill: None,
            stroke: Some(Stroke::new(1.0, Color::WHITE)),
        }],
    };
    assert_eq!(
        list.validate(),
        Err(DrawError::TooManyPoints {
            index: 0,
            limit: MAX_PATH_POINTS,
        })
    );
}

#[test]
fn excessive_command_count_is_rejected() {
    let list = DrawList {
        commands: vec![DrawCommand::PopClip; MAX_COMMANDS + 1],
    };
    assert_eq!(
        list.validate(),
        Err(DrawError::TooManyCommands {
            limit: MAX_COMMANDS
        })
    );
}
