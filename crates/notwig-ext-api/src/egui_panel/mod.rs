//! Guest-owned egui panels.
//!
//! The wire types in this module are deliberately independent of `egui`'s
//! internal serialization format. Extensions run a private `egui::Context` and
//! send only tessellated meshes, texture deltas, and validated host actions.
//! The host decodes those bytes, remaps texture ids, and composites the meshes.

#[cfg(feature = "egui")]
mod bridge;

#[cfg(feature = "egui")]
pub use bridge::{apply_theme, run_frame, set_param_normalized, set_param_value};

use serde::{Deserialize, Serialize};

use crate::panel::{PanelAction, validate_action};
use crate::ui::CursorIcon;

/// Maximum serialized size of one egui frame.
pub const MAX_EGUI_FRAME_BYTES: usize = 8 * 1024 * 1024;
/// Maximum number of meshes in one frame.
pub const MAX_EGUI_MESHES: usize = 8 * 1024;
/// Maximum number of vertices across all meshes in one frame.
pub const MAX_EGUI_VERTICES: usize = 262_144;
/// Maximum number of indices across all meshes in one frame.
pub const MAX_EGUI_INDICES: usize = 786_432;
/// Maximum number of texture deltas in one frame.
pub const MAX_EGUI_TEXTURES: usize = 256;
/// Maximum width or height of one texture.
pub const MAX_EGUI_TEXTURE_DIM: u32 = 8 * 1024;
/// Maximum total texels uploaded in one frame.
pub const MAX_EGUI_TEXTURE_TEXELS: usize = 16 * 1024 * 1024;
/// Maximum number of input events in one frame.
pub const MAX_EGUI_EVENTS: usize = 2 * 1024;
/// Maximum size of one text event or copied string.
pub const MAX_EGUI_TEXT_BYTES: usize = 64 * 1024;
/// Maximum number of host actions in one frame.
pub const MAX_EGUI_ACTIONS: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct EguiModifiers {
    pub alt: bool,
    pub ctrl: bool,
    pub shift: bool,
    pub mac_cmd: bool,
    pub command: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EguiPointerButton {
    Primary,
    Secondary,
    Middle,
}

/// Serializable pointer, keyboard, and IME input for one panel frame.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EguiInput {
    pub width: f32,
    pub height: f32,
    pub pixels_per_point: f32,
    pub time: Option<f64>,
    pub focused: bool,
    pub max_texture_side: u32,
    pub modifiers: EguiModifiers,
    pub events: Vec<EguiEvent>,
}

impl EguiInput {
    pub fn validate(&self) -> Result<(), EguiFrameError> {
        if !self.width.is_finite()
            || !self.height.is_finite()
            || self.width <= 0.0
            || self.height <= 0.0
        {
            return Err(EguiFrameError::InvalidInput("panel size"));
        }
        if !self.pixels_per_point.is_finite() || self.pixels_per_point <= 0.0 {
            return Err(EguiFrameError::InvalidInput("pixels-per-point"));
        }
        if self.time.is_some_and(|time| !time.is_finite()) {
            return Err(EguiFrameError::InvalidInput("time"));
        }
        if self.max_texture_side == 0 || self.max_texture_side > MAX_EGUI_TEXTURE_DIM {
            return Err(EguiFrameError::InvalidInput("max texture side"));
        }
        if self.events.len() > MAX_EGUI_EVENTS {
            return Err(EguiFrameError::TooManyEvents {
                limit: MAX_EGUI_EVENTS,
            });
        }
        for event in &self.events {
            event.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EguiEvent {
    PointerMoved {
        pos: [f32; 2],
    },
    PointerButton {
        pos: [f32; 2],
        button: EguiPointerButton,
        pressed: bool,
        modifiers: EguiModifiers,
    },
    PointerGone,
    MouseWheel {
        delta: [f32; 2],
        modifiers: EguiModifiers,
    },
    Key {
        key: String,
        pressed: bool,
        repeat: bool,
        modifiers: EguiModifiers,
    },
    Text(String),
    Copy,
    Cut,
    Paste(String),
    WindowFocused(bool),
}

impl EguiEvent {
    fn validate(&self) -> Result<(), EguiFrameError> {
        match self {
            Self::PointerMoved { pos } | Self::PointerButton { pos, .. } => {
                if !finite_pair(*pos) {
                    return Err(EguiFrameError::InvalidInput("pointer"));
                }
            }
            Self::MouseWheel { delta, .. } => {
                if !finite_pair(*delta) {
                    return Err(EguiFrameError::InvalidInput("scroll"));
                }
            }
            Self::Key { key, .. } => {
                if key.len() > 64 {
                    return Err(EguiFrameError::InvalidInput("key name"));
                }
            }
            Self::Text(text) | Self::Paste(text) => {
                if text.len() > MAX_EGUI_TEXT_BYTES {
                    return Err(EguiFrameError::TextTooLong {
                        limit: MAX_EGUI_TEXT_BYTES,
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
pub struct EguiPrimitive {
    pub clip_rect: [f32; 4],
    pub mesh: EguiMesh,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EguiMesh {
    pub indices: Vec<u32>,
    pub vertices: Vec<EguiVertex>,
    pub texture_id: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EguiVertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub color: [u8; 4],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct EguiTextureOptions {
    pub linear: bool,
    pub repeat: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EguiImage {
    pub width: u32,
    pub height: u32,
    /// Premultiplied sRGBA pixels packed as little-endian `u32`.
    pub pixels: Vec<u32>,
}

impl EguiImage {
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
pub struct EguiTextureDelta {
    pub id: u64,
    pub pos: Option<[u32; 2]>,
    pub options: EguiTextureOptions,
    pub image: EguiImage,
}

/// One validated guest egui frame.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EguiFrame {
    pub primitives: Vec<EguiPrimitive>,
    pub textures: Vec<EguiTextureDelta>,
    pub free_textures: Vec<u64>,
    pub actions: Vec<PanelAction>,
    pub cursor: Option<CursorIcon>,
    pub wants_keyboard: bool,
    pub wants_pointer: bool,
    pub copy_text: Option<String>,
    pub repaint_after_ms: Option<u32>,
}

impl EguiFrame {
    pub fn validate(&self) -> Result<(), EguiFrameError> {
        if self.primitives.len() > MAX_EGUI_MESHES {
            return Err(EguiFrameError::TooManyMeshes {
                limit: MAX_EGUI_MESHES,
            });
        }
        if self.textures.len() > MAX_EGUI_TEXTURES {
            return Err(EguiFrameError::TooManyTextures {
                limit: MAX_EGUI_TEXTURES,
            });
        }
        if self.actions.len() > MAX_EGUI_ACTIONS {
            return Err(EguiFrameError::TooManyActions {
                limit: MAX_EGUI_ACTIONS,
            });
        }
        if let Some(text) = &self.copy_text {
            if text.len() > MAX_EGUI_TEXT_BYTES {
                return Err(EguiFrameError::TextTooLong {
                    limit: MAX_EGUI_TEXT_BYTES,
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
        if vertices > MAX_EGUI_VERTICES {
            return Err(EguiFrameError::TooManyVertices {
                limit: MAX_EGUI_VERTICES,
            });
        }
        if indices > MAX_EGUI_INDICES {
            return Err(EguiFrameError::TooManyIndices {
                limit: MAX_EGUI_INDICES,
            });
        }

        let mut texels = 0usize;
        for (index, texture) in self.textures.iter().enumerate() {
            if texture.image.width == 0
                || texture.image.height == 0
                || texture.image.width > MAX_EGUI_TEXTURE_DIM
                || texture.image.height > MAX_EGUI_TEXTURE_DIM
            {
                return Err(EguiFrameError::InvalidTexture {
                    index,
                    reason: "dimensions",
                });
            }
            let size = texture.image.texels();
            if size == usize::MAX || size != texture.image.pixels.len() {
                return Err(EguiFrameError::InvalidTexture {
                    index,
                    reason: "pixel count",
                });
            }
            if let Some(pos) = texture.pos {
                let overflow = pos[0] > texture.image.width || pos[1] > texture.image.height;
                if overflow {
                    return Err(EguiFrameError::InvalidTexture {
                        index,
                        reason: "patch position",
                    });
                }
            }
            texels = texels.saturating_add(size);
        }
        if texels > MAX_EGUI_TEXTURE_TEXELS {
            return Err(EguiFrameError::TextureBudget {
                limit: MAX_EGUI_TEXTURE_TEXELS,
            });
        }

        for (index, action) in self.actions.iter().enumerate() {
            validate_action(action, index).map_err(|_| EguiFrameError::InvalidAction { index })?;
        }
        Ok(())
    }
}

impl EguiMesh {
    fn validate(&self, index: usize) -> Result<(), EguiFrameError> {
        if self.vertices.len() > MAX_EGUI_VERTICES {
            return Err(EguiFrameError::TooManyVertices {
                limit: MAX_EGUI_VERTICES,
            });
        }
        if self.indices.len() > MAX_EGUI_INDICES {
            return Err(EguiFrameError::TooManyIndices {
                limit: MAX_EGUI_INDICES,
            });
        }
        if self.indices.len() % 3 != 0 {
            return Err(EguiFrameError::InvalidMesh {
                index,
                reason: "index count",
            });
        }
        for vertex in &self.vertices {
            if !finite_pair(vertex.pos) || !finite_pair(vertex.uv) {
                return Err(EguiFrameError::InvalidMesh {
                    index,
                    reason: "non-finite vertex",
                });
            }
        }
        for &vertex_index in &self.indices {
            if usize::try_from(vertex_index)
                .map_or(true, |vertex_index| vertex_index >= self.vertices.len())
            {
                return Err(EguiFrameError::InvalidMesh {
                    index,
                    reason: "index out of range",
                });
            }
        }
        Ok(())
    }
}

fn validate_clip(clip: [f32; 4], index: usize) -> Result<(), EguiFrameError> {
    if !clip.into_iter().all(f32::is_finite) {
        return Err(EguiFrameError::InvalidMesh {
            index,
            reason: "non-finite clip",
        });
    }
    if clip[0] > clip[2] || clip[1] > clip[3] {
        return Err(EguiFrameError::InvalidMesh {
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
pub enum EguiFrameError {
    #[error("invalid egui input: {0}")]
    InvalidInput(&'static str),
    #[error("egui frame exceeds {limit} meshes")]
    TooManyMeshes { limit: usize },
    #[error("egui frame exceeds {limit} vertices")]
    TooManyVertices { limit: usize },
    #[error("egui frame exceeds {limit} indices")]
    TooManyIndices { limit: usize },
    #[error("egui frame exceeds {limit} textures")]
    TooManyTextures { limit: usize },
    #[error("egui frame exceeds {limit} texture texels")]
    TextureBudget { limit: usize },
    #[error("egui frame exceeds {limit} events")]
    TooManyEvents { limit: usize },
    #[error("egui frame exceeds {limit} actions")]
    TooManyActions { limit: usize },
    #[error("egui text exceeds {limit} bytes")]
    TextTooLong { limit: usize },
    #[error("mesh {index} is invalid: {reason}")]
    InvalidMesh { index: usize, reason: &'static str },
    #[error("texture {index} is invalid: {reason}")]
    InvalidTexture { index: usize, reason: &'static str },
    #[error("action {index} is invalid")]
    InvalidAction { index: usize },
}
