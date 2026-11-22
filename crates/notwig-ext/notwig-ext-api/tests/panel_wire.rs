use notwig_ext_api::{
    PanelAction, PanelFrame, PanelImage, PanelInput, PanelMesh, PanelModifiers, PanelPrimitive,
    PanelTextureDelta, PanelTextureOptions, PanelVertex,
};

fn valid_frame() -> PanelFrame {
    PanelFrame {
        primitives: vec![PanelPrimitive {
            clip_rect: [0.0, 0.0, 100.0, 100.0],
            mesh: PanelMesh {
                indices: vec![0, 1, 2],
                vertices: vec![
                    PanelVertex {
                        pos: [0.0, 0.0],
                        uv: [0.0, 0.0],
                        color: [255; 4],
                    },
                    PanelVertex {
                        pos: [10.0, 0.0],
                        uv: [1.0, 0.0],
                        color: [255; 4],
                    },
                    PanelVertex {
                        pos: [0.0, 10.0],
                        uv: [0.0, 1.0],
                        color: [255; 4],
                    },
                ],
                texture_id: 0,
            },
        }],
        textures: vec![PanelTextureDelta {
            id: 0,
            pos: None,
            options: PanelTextureOptions {
                linear: true,
                repeat: false,
            },
            image: PanelImage {
                width: 1,
                height: 1,
                pixels: vec![u32::MAX],
            },
        }],
        free_textures: Vec::new(),
        actions: vec![PanelAction::RequestRepaint],
        cursor: None,
        wants_keyboard: false,
        wants_pointer: false,
        copy_text: None,
        repaint_after_ms: Some(16),
    }
}

#[test]
fn frame_round_trips_through_postcard() {
    let frame = valid_frame();
    frame.validate().unwrap();
    let bytes = postcard::to_allocvec(&frame).unwrap();
    let decoded: PanelFrame = postcard::from_bytes(&bytes).unwrap();
    assert_eq!(decoded, frame);
}

#[test]
fn invalid_index_is_rejected() {
    let mut frame = valid_frame();
    frame.primitives[0].mesh.indices = vec![0, 1, 4];
    assert!(frame.validate().is_err());
}

#[test]
fn invalid_texture_size_is_rejected() {
    let mut frame = valid_frame();
    frame.textures[0].image.width = 2;
    frame.textures[0].image.height = 2;
    assert!(frame.validate().is_err());
}

#[test]
fn non_finite_geometry_is_rejected() {
    let mut frame = valid_frame();
    frame.primitives[0].mesh.vertices[0].pos[0] = f32::NAN;
    assert!(frame.validate().is_err());
}

#[test]
fn input_limits_are_enforced() {
    let mut input = PanelInput {
        width: 100.0,
        height: 100.0,
        pixels_per_point: 1.0,
        time: Some(0.0),
        focused: true,
        max_texture_side: 4096,
        modifiers: PanelModifiers::default(),
        events: Vec::new(),
    };
    input.validate().unwrap();
    input.width = 0.0;
    assert!(input.validate().is_err());
}
