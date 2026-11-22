//! Guest-owned extension panels.
//!
//! The wire types in this module are deliberately independent of `egui`'s
//! internal serialization format. Extensions run a private `egui::Context` and
//! send only tessellated meshes, texture deltas, and validated host actions.
//! The host decodes those bytes, remaps texture ids, and composites the meshes.

mod bridge;

pub use bridge::{apply_theme, run_frame, set_param_normalized, set_param_value};

use serde::{Deserialize, Serialize};

use crate::cursor::CursorIcon;
use crate::panel::{PanelAction, validate_action};

/// Maximum serialized size of one panel frame.
pub const MAX_PANEL_FRAME_BYTES: usize = 8 * 1024 * 1024;
/// Maximum number of meshes in one frame.
pub const MAX_PANEL_MESHES: usize = 8 * 1024;
/// Maximum number of vertices across all meshes in one frame.
pub const MAX_PANEL_VERTICES: usize = 262_144;
/// Maximum number of indices across all meshes in one frame.
pub const MAX_PANEL_INDICES: usize = 786_432;
/// Maximum number of texture deltas in one frame.
pub const MAX_PANEL_TEXTURES: usize = 256;
/// Maximum width or height of one texture.
pub const MAX_PANEL_TEXTURE_DIM: u32 = 8 * 1024;
/// Maximum total texels uploaded in one frame.
pub const MAX_PANEL_TEXTURE_TEXELS: usize = 16 * 1024 * 1024;
/// Maximum number of input events in one frame.
pub const MAX_PANEL_EVENTS: usize = 2 * 1024;
/// Maximum size of one text event or copied string.
pub const MAX_PANEL_TEXT_BYTES: usize = 64 * 1024;
/// Maximum number of host actions in one frame.
pub const MAX_PANEL_ACTIONS: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PanelModifiers {
    pub alt: bool,
    pub ctrl: bool,
    pub shift: bool,
    pub mac_cmd: bool,
    pub command: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PanelPointerButton {
    Primary,
    Secondary,
    Middle,
}

/// Serializable pointer, keyboard, and IME input for one panel frame.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PanelInput {
    pub width: f32,
    pub height: f32,
    pub pixels_per_point: f32,
    pub time: Option<f64>,
    pub focused: bool,
    pub max_texture_side: u32,
    pub modifiers: PanelModifiers,
    pub events: Vec<PanelEvent>,
}

impl PanelInput {
    pub fn validate(&self) -> Result<(), PanelFrameError> {
        if !self.width.is_finite()
            || !self.height.is_finite()
            || self.width <= 0.0
            || self.height <= 0.0
        {
            return Err(PanelFrameError::InvalidInput("panel size"));
        }
        if !self.pixels_per_point.is_finite() || self.pixels_per_point <= 0.0 {
            return Err(PanelFrameError::InvalidInput("pixels-per-point"));
        }
        if self.time.is_some_and(|time| !time.is_finite()) {
            return Err(PanelFrameError::InvalidInput("time"));
        }
        if self.max_texture_side == 0 || self.max_texture_side > MAX_PANEL_TEXTURE_DIM {
            return Err(PanelFrameError::InvalidInput("max texture side"));
        }
        if self.events.len() > MAX_PANEL_EVENTS {
            return Err(PanelFrameError::TooManyEvents {
                limit: MAX_PANEL_EVENTS,
            });
        }
        for event in &self.events {
            event.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PanelEvent {
    PointerMoved {
        pos: [f32; 2],
    },
    PointerButton {
        pos: [f32; 2],
        button: PanelPointerButton,
        pressed: bool,
        modifiers: PanelModifiers,
    },
    PointerGone,
    MouseWheel {
        delta: [f32; 2],
        modifiers: PanelModifiers,
    },
    Key {
        key: String,
        pressed: bool,
        repeat: bool,
        modifiers: PanelModifiers,
    },
    Text(String),
    Copy,
    Cut,
    Paste(String),
    WindowFocused(bool),
}

impl PanelEvent {
    fn validate(&self) -> Result<(), PanelFrameError> {
        match self {
            Self::PointerMoved { pos } | Self::PointerButton { pos, .. } => {
                if !finite_pair(*pos) {
                    return Err(PanelFrameError::InvalidInput("pointer"));
                }
            }
            Self::MouseWheel { delta, .. } => {
                if !finite_pair(*delta) {
                    return Err(PanelFrameError::InvalidInput("scroll"));
                }
            }
            Self::Key { key, .. } => {
                if key.len() > 64 {
                    return Err(PanelFrameError::InvalidInput("key name"));
                }
            }
            Self::Text(text) | Self::Paste(text) => {
                if text.len() > MAX_PANEL_TEXT_BYTES {
                    return Err(PanelFrameError::TextTooLong {
                        limit: MAX_PANEL_TEXT_BYTES,
                    });
                }
            }
            Self::PointerGone | Self::Copy | Self::Cut | Self::WindowFocused(_) => {}
        }
        Ok(())
    }
}

/// One tessellated mesh and its guest clip rectangle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PanelPrimitive {
    pub clip_rect: [f32; 4],
    pub mesh: PanelMesh,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PanelMesh {
    pub indices: Vec<u32>,
    pub vertices: Vec<PanelVertex>,
    pub texture_id: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PanelVertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub color: [u8; 4],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PanelTextureOptions {
    pub linear: bool,
    pub repeat: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PanelImage {
    pub width: u32,
    pub height: u32,
    /// Premultiplied sRGBA pixels packed as little-endian `u32`.
    pub pixels: Vec<u32>,
}

impl PanelImage {
    #[must_use]
    pub fn texels(&self) -> usize {
        usize::try_from(self.width)
            .ok()
            .and_then(|width| {
                usize::try_from(self.height)
                    .ok()
                    .and_then(|height| width.checked_mul(height))
            })
            .unwrap_or(usize::MAX)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PanelTextureDelta {
    pub id: u64,
    pub pos: Option<[u32; 2]>,
    pub options: PanelTextureOptions,
    pub image: PanelImage,
}

/// One validated guest panel frame.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PanelFrame {
    pub primitives: Vec<PanelPrimitive>,
    pub textures: Vec<PanelTextureDelta>,
    pub free_textures: Vec<u64>,
    pub actions: Vec<PanelAction>,
    pub cursor: Option<CursorIcon>,
    pub wants_keyboard: bool,
    pub wants_pointer: bool,
    pub copy_text: Option<String>,
    pub repaint_after_ms: Option<u32>,
}

impl PanelFrame {
    pub fn validate(&self) -> Result<(), PanelFrameError> {
        if self.primitives.len() > MAX_PANEL_MESHES {
            return Err(PanelFrameError::TooManyMeshes {
                limit: MAX_PANEL_MESHES,
            });
        }
        if self.textures.len() > MAX_PANEL_TEXTURES {
            return Err(PanelFrameError::TooManyTextures {
                limit: MAX_PANEL_TEXTURES,
            });
        }
        if self.actions.len() > MAX_PANEL_ACTIONS {
            return Err(PanelFrameError::TooManyActions {
                limit: MAX_PANEL_ACTIONS,
            });
        }
        if let Some(text) = &self.copy_text {
            if text.len() > MAX_PANEL_TEXT_BYTES {
                return Err(PanelFrameError::TextTooLong {
                    limit: MAX_PANEL_TEXT_BYTES,
                });
            }
        }

        let mut vertices = 0usize;
        let mut indices = 0usize;
        for (index, primitive) in self.primitives.iter().enumerate() {
            validate_clip(primitive.clip_rect, index)?;
            primitive.mesh.validate(index)?;
            vertices = vertices.saturating_add(primitive.mesh.vertices.len());
            indices = indices.saturating_add(primitive.mesh.indices.len());
        }
        if vertices > MAX_PANEL_VERTICES {
            return Err(PanelFrameError::TooManyVertices {
                limit: MAX_PANEL_VERTICES,
            });
        }
        if indices > MAX_PANEL_INDICES {
            return Err(PanelFrameError::TooManyIndices {
                limit: MAX_PANEL_INDICES,
            });
        }

        let mut texels = 0usize;
        for (index, texture) in self.textures.iter().enumerate() {
            if texture.image.width == 0
                || texture.image.height == 0
                || texture.image.width > MAX_PANEL_TEXTURE_DIM
                || texture.image.height > MAX_PANEL_TEXTURE_DIM
            {
                return Err(PanelFrameError::InvalidTexture {
                    index,
                    reason: "dimensions",
                });
            }
            let size = texture.image.texels();
            if size == usize::MAX || size != texture.image.pixels.len() {
                return Err(PanelFrameError::InvalidTexture {
                    index,
                    reason: "pixel count",
                });
            }
            if let Some(pos) = texture.pos {
                let overflow = pos[0] > texture.image.width || pos[1] > texture.image.height;
                if overflow {
                    return Err(PanelFrameError::InvalidTexture {
                        index,
                        reason: "patch position",
                    });
                }
            }
            texels = texels.saturating_add(size);
        }
        if texels > MAX_PANEL_TEXTURE_TEXELS {
            return Err(PanelFrameError::TextureBudget {
                limit: MAX_PANEL_TEXTURE_TEXELS,
            });
        }

        for (index, action) in self.actions.iter().enumerate() {
            validate_action(action).map_err(|_| PanelFrameError::InvalidAction { index })?;
        }
        Ok(())
    }
}

impl PanelMesh {
    fn validate(&self, index: usize) -> Result<(), PanelFrameError> {
        if self.vertices.len() > MAX_PANEL_VERTICES {
            return Err(PanelFrameError::TooManyVertices {
                limit: MAX_PANEL_VERTICES,
            });
        }
        if self.indices.len() > MAX_PANEL_INDICES {
            return Err(PanelFrameError::TooManyIndices {
                limit: MAX_PANEL_INDICES,
            });
        }
        if self.indices.len() % 3 != 0 {
            return Err(PanelFrameError::InvalidMesh {
                index,
                reason: "index count",
            });
        }
        for vertex in &self.vertices {
            if !finite_pair(vertex.pos) || !finite_pair(vertex.uv) {
                return Err(PanelFrameError::InvalidMesh {
                    index,
                    reason: "non-finite vertex",
                });
            }
        }
        for &vertex_index in &self.indices {
            if usize::try_from(vertex_index)
                .map_or(true, |vertex_index| vertex_index >= self.vertices.len())
            {
                return Err(PanelFrameError::InvalidMesh {
                    index,
                    reason: "index out of range",
                });
            }
        }
        Ok(())
    }
}

fn validate_clip(clip: [f32; 4], index: usize) -> Result<(), PanelFrameError> {
    if !clip.into_iter().all(f32::is_finite) {
        return Err(PanelFrameError::InvalidMesh {
            index,
            reason: "non-finite clip",
        });
    }
    if clip[0] > clip[2] || clip[1] > clip[3] {
        return Err(PanelFrameError::InvalidMesh {
            index,
            reason: "inverted clip",
        });
    }
    Ok(())
}

fn finite_pair(value: [f32; 2]) -> bool {
    value.into_iter().all(f32::is_finite)
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum PanelFrameError {
    #[error("invalid panel input: {0}")]
    InvalidInput(&'static str),
    #[error("panel frame exceeds {limit} meshes")]
    TooManyMeshes { limit: usize },
    #[error("panel frame exceeds {limit} vertices")]
    TooManyVertices { limit: usize },
    #[error("panel frame exceeds {limit} indices")]
    TooManyIndices { limit: usize },
    #[error("panel frame exceeds {limit} textures")]
    TooManyTextures { limit: usize },
    #[error("panel frame exceeds {limit} texture texels")]
    TextureBudget { limit: usize },
    #[error("panel frame exceeds {limit} events")]
    TooManyEvents { limit: usize },
    #[error("panel frame exceeds {limit} actions")]
    TooManyActions { limit: usize },
    #[error("panel text exceeds {limit} bytes")]
    TextTooLong { limit: usize },
    #[error("mesh {index} is invalid: {reason}")]
    InvalidMesh { index: usize, reason: &'static str },
    #[error("texture {index} is invalid: {reason}")]
    InvalidTexture { index: usize, reason: &'static str },
    #[error("action {index} is invalid")]
    InvalidAction { index: usize },
}
