//! Versioned command contribution contract.
//!
//! A command reads a bounded [`CommandContext`] snapshot and returns a
//! [`CommandEffect`]. The host owns validation, command lowering, selection,
//! and undo. Commands never receive the project document or a mutable UI.

use crate::extension_point::ExtensionPoint;
use notwig_core::sequencer::{AudioSlice, NoteEntity};
use notwig_core::{
    ClipId, Db, DeviceId, NoteId, Pan, ParamId, ParamValue, RangeMus, TimeMus, TrackId, time::Color,
};
use serde::{Deserialize, Serialize};

/// Marker interface for command extension points.
pub trait CommandInterface {}

/// Built-in extension point for command implementations.
pub const EP_COMMANDS: ExtensionPoint<dyn CommandInterface> =
    ExtensionPoint::new("notwig.commands");

/// Legacy manifest point id used for command contributions.
pub const COMMAND_POINT: &str = "notwig.command";
/// Contract id and major version for command contributions.
pub const COMMAND_CONTRACT: &str = "notwig.command@1";
/// Maximum number of selected notes copied into a command context.
pub const MAX_COMMAND_CONTEXT_NOTES: usize = 4096;
/// Maximum number of selected audio slices copied into a command context.
pub const MAX_COMMAND_CONTEXT_SLICES: usize = 4096;
/// Maximum number of edits one command effect may request.
pub const MAX_COMMAND_EDITS: usize = 256;
/// Maximum length of a command description.
pub const MAX_COMMAND_DESCRIPTION_BYTES: usize = 512;
/// Maximum length of a `when` expression.
pub const MAX_COMMAND_WHEN_BYTES: usize = 512;

/// Stable command identity, namespaced by the built-in or extension owner.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CommandId(pub String);

impl CommandId {
    /// Creates a command id from a string.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Returns the underlying string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for CommandId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Resolved command metadata used by the palette, shortcuts, and executor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandDescriptor {
    /// Stable command id.
    pub id: String,
    /// Palette title.
    pub title: String,
    /// Palette category, such as `MIDI` or `Edit`.
    pub category: String,
    /// Short description.
    pub description: String,
    /// Optional declarative enablement expression.
    pub when: Option<String>,
    /// Suggested keybindings as config strings such as `Cmd+Shift+P`.
    pub default_bindings: Vec<String>,
    /// Owning extension id, or `None` for built-ins.
    pub extension_id: Option<String>,
}

impl CommandDescriptor {
    /// Returns whether this command is enabled for `context`.
    ///
    /// An invalid expression is disabled rather than treated as enabled.
    #[must_use]
    pub fn enabled(&self, context: &CommandContext) -> bool {
        self.when
            .as_deref()
            .is_none_or(|expr| evaluate_when(expr, context).unwrap_or(false))
    }
}

/// Editor surface that owns focus when a command runs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandFocus {
    /// Arranger/timeline.
    #[default]
    Timeline,
    /// Audio clip editor.
    AudioEditor,
    /// MIDI clip editor.
    MidiEditor,
    /// Device editor.
    DeviceEditor,
    /// Extension panel.
    Panel,
}

impl CommandFocus {
    /// Canonical expression string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Timeline => "timeline",
            Self::AudioEditor => "audio_editor",
            Self::MidiEditor => "midi_editor",
            Self::DeviceEditor => "device_editor",
            Self::Panel => "panel",
        }
    }
}

/// A time selection included in a command context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandTimeSelection {
    /// Selected time range.
    pub range: RangeMus,
    /// Lanes included in the selection.
    pub tracks: Vec<TrackId>,
}

/// Bounded selection snapshot supplied to a command.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CommandSelection {
    /// Selected tracks.
    pub tracks: Vec<TrackId>,
    /// Selected clips.
    pub clips: Vec<ClipId>,
    /// Selected devices.
    pub devices: Vec<DeviceId>,
    /// Optional time selection.
    pub time: Option<CommandTimeSelection>,
    /// Selected notes from the active MIDI clip.
    pub notes: Vec<NoteEntity>,
    /// Selected slices from the active audio clip.
    pub slices: Vec<AudioSlice>,
}

/// Immutable, bounded snapshot supplied to a command handler.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommandContext {
    /// Document revision at snapshot time.
    pub revision: u64,
    /// Focused editor surface.
    pub focus: CommandFocus,
    /// Bounded selection.
    pub selection: CommandSelection,
    /// Active track, when there is one.
    pub active_track: Option<TrackId>,
    /// Active clip, when an editor has one.
    pub active_clip: Option<ClipId>,
    /// Active device, when the device editor has one.
    pub active_device: Option<DeviceId>,
    /// Current playhead.
    pub playhead: TimeMus,
    /// Project tempo in BPM.
    pub tempo_bpm: f64,
    /// Engine sample rate.
    pub sample_rate: u32,
    /// Whether snapping is currently enabled.
    pub snap_enabled: bool,
}

impl Default for CommandContext {
    fn default() -> Self {
        Self {
            revision: 0,
            focus: CommandFocus::Timeline,
            selection: CommandSelection::default(),
            active_track: None,
            active_clip: None,
            active_device: None,
            playhead: TimeMus::zero(),
            tempo_bpm: 120.0,
            sample_rate: 44_100,
            snap_enabled: true,
        }
    }
}

/// One host-owned edit family a command may request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CommandEdit {
    /// Set or remove notes in a MIDI clip.
    MidiNotes {
        /// Target clip.
        clip_id: ClipId,
        /// Notes to upsert.
        notes: Vec<NoteEntity>,
        /// Note ids to remove.
        removed: Vec<NoteId>,
    },
    /// Replace the slices of an audio clip.
    AudioSlices {
        /// Target clip.
        clip_id: ClipId,
        /// New slice list.
        slices: Vec<AudioSlice>,
    },
    /// Update host-owned audio clip curves.
    ///
    /// The outer option selects whether the curve changes. The inner option
    /// sets the new curve, or clears it with `None`.
    AudioCurves {
        /// Target clip.
        clip_id: ClipId,
        /// Gain curve change, when requested.
        #[serde(default)]
        gain_curve: Option<Option<notwig_core::Curve>>,
        /// Pan curve change, when requested.
        #[serde(default)]
        pan_curve: Option<Option<notwig_core::Curve>>,
    },
    /// Update clip fields.
    ClipFields {
        /// Target clip.
        clip_id: ClipId,
        /// New range.
        range: Option<RangeMus>,
        /// New name.
        name: Option<Option<String>>,
        /// New muted state.
        muted: Option<bool>,
        /// New color.
        color: Option<Option<Color>>,
    },
    /// Update a track mixer.
    TrackMixer {
        /// Target track.
        track_id: TrackId,
        /// New volume.
        volume_db: Option<Db>,
        /// New pan.
        pan: Option<Pan>,
        /// New mute state.
        mute: Option<bool>,
        /// New solo state.
        solo: Option<bool>,
    },
    /// Set one device parameter.
    DeviceParam {
        /// Target device.
        device_id: DeviceId,
        /// Parameter id.
        param_id: ParamId,
        /// Parameter value.
        value: ParamValue,
    },
}

/// One host-owned UI request a command may return.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandUiRequest {
    /// Show a notification.
    Notify {
        /// Message text.
        message: String,
    },
    /// Focus an editor, optionally opening a clip.
    FocusEditor {
        /// Target surface.
        focus: CommandFocus,
        /// Clip to open for an audio/MIDI editor.
        clip_id: Option<ClipId>,
    },
}

/// Result returned by a command handler.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CommandEffect {
    /// Do nothing.
    None,
    /// Apply edits and/or UI requests through the host.
    Apply {
        /// Validated edit families.
        edits: Vec<CommandEdit>,
        /// Validated UI requests.
        ui: Vec<CommandUiRequest>,
    },
    /// Report a typed failure.
    Error {
        /// Human-readable failure reason.
        message: String,
    },
}

/// Author-facing command implementation.
pub trait CommandExtension: Send + Sized + 'static {
    /// Creates a command instance.
    fn new() -> Self;

    /// Returns the command metadata used in the palette and shortcut editor.
    fn descriptor() -> CommandDescriptor;

    /// Executes the command against a bounded context.
    ///
    /// The handler must be deterministic for the supplied context unless it
    /// explicitly carries a user seed. It must not perform I/O or block.
    fn execute(&mut self, context: &CommandContext) -> CommandEffect;
}

/// Validation failure for a declarative `when` expression.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid command condition: {message}")]
pub struct WhenError {
    /// Human-readable failure reason.
    pub message: String,
}

impl WhenError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// Evaluates a declarative command condition.
///
/// Supported fields are `focus` (string), `selection.notes`,
/// `selection.clips`, `selection.devices`, `selection.slices`, `selection.time`
/// (bool), `revision`, `snap` (bool), and `playing` is intentionally absent.
pub fn evaluate_when(expr: &str, context: &CommandContext) -> Result<bool, WhenError> {
    if expr.len() > MAX_COMMAND_WHEN_BYTES {
        return Err(WhenError::new("expression is too long"));
    }
    let mut parser = WhenParser::new(expr)?;
    let result = parser.parse_or(context)?;
    if parser.peek().is_some() {
        return Err(WhenError::new("unexpected trailing tokens"));
    }
    Ok(result)
}

/// Parses and validates a condition without a context.
pub fn validate_when(expr: &str) -> Result<(), WhenError> {
    evaluate_when(expr, &CommandContext::default()).map(|_| ())
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Ident(String),
    Number(i64),
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
    And,
    Or,
    Not,
    LParen,
    RParen,
}

struct WhenParser {
    tokens: Vec<Token>,
    index: usize,
}

impl WhenParser {
    fn new(expr: &str) -> Result<Self, WhenError> {
        Ok(Self {
            tokens: tokenize(expr)?,
            index: 0,
        })
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.index)
    }

    fn next(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.index).cloned();
        self.index += usize::from(token.is_some());
        token
    }

    fn eat(&mut self, token: &Token) -> bool {
        if self.peek() == Some(token) {
            self.index += 1;
            true
        } else {
            false
        }
    }

    fn parse_or(&mut self, context: &CommandContext) -> Result<bool, WhenError> {
        let mut value = self.parse_and(context)?;
        while self.eat(&Token::Or) {
            let right = self.parse_and(context)?;
            value = value || right;
        }
        Ok(value)
    }

    fn parse_and(&mut self, context: &CommandContext) -> Result<bool, WhenError> {
        let mut value = self.parse_unary(context)?;
        while self.eat(&Token::And) {
            let right = self.parse_unary(context)?;
            value = value && right;
        }
        Ok(value)
    }

    fn parse_unary(&mut self, context: &CommandContext) -> Result<bool, WhenError> {
        if self.eat(&Token::Not) {
            return self.parse_unary(context).map(|value| !value);
        }
        if self.eat(&Token::LParen) {
            let value = self.parse_or(context)?;
            if !self.eat(&Token::RParen) {
                return Err(WhenError::new("missing closing parenthesis"));
            }
            return Ok(value);
        }
        self.parse_atom(context)
    }

    fn parse_atom(&mut self, context: &CommandContext) -> Result<bool, WhenError> {
        let Token::Ident(path) = self
            .next()
            .ok_or_else(|| WhenError::new("expected a field"))?
        else {
            return Err(WhenError::new("expected a field"));
        };
        let Some(operator) = self.next() else {
            return Self::boolean_path(&path, context);
        };
        let value = match self.next() {
            Some(Token::Number(value)) => Literal::Number(value),
            Some(Token::Ident(value)) => Literal::Ident(value),
            _ => return Err(WhenError::new("expected a value after the operator")),
        };
        if path == "focus" {
            let Literal::Ident(value) = value else {
                return Err(WhenError::new("focus compares with a named value"));
            };
            let matches = context.focus.as_str() == value;
            return match operator {
                Token::Eq => Ok(matches),
                Token::Ne => Ok(!matches),
                _ => Err(WhenError::new("focus only supports == and !=")),
            };
        }
        let Literal::Number(right) = value else {
            return Err(WhenError::new("numeric field compares with a number"));
        };
        let left = Self::number_path(&path, context)?;
        let result = match operator {
            Token::Eq => left == right,
            Token::Ne => left != right,
            Token::Gt => left > right,
            Token::Ge => left >= right,
            Token::Lt => left < right,
            Token::Le => left <= right,
            _ => return Err(WhenError::new("unsupported operator")),
        };
        Ok(result)
    }

    fn boolean_path(path: &str, context: &CommandContext) -> Result<bool, WhenError> {
        match path {
            "selection.time" => Ok(context.selection.time.is_some()),
            "snap" => Ok(context.snap_enabled),
            "playing" => Err(WhenError::new("unknown field playing")),
            "focus" => Err(WhenError::new("focus requires a comparison")),
            _ => Err(WhenError::new(format!("unknown field {path}"))),
        }
    }

    fn number_path(path: &str, context: &CommandContext) -> Result<i64, WhenError> {
        match path {
            "selection.notes" => {
                Ok(i64::try_from(context.selection.notes.len()).unwrap_or(i64::MAX))
            }
            "selection.clips" => {
                Ok(i64::try_from(context.selection.clips.len()).unwrap_or(i64::MAX))
            }
            "selection.devices" => {
                Ok(i64::try_from(context.selection.devices.len()).unwrap_or(i64::MAX))
            }
            "selection.slices" => {
                Ok(i64::try_from(context.selection.slices.len()).unwrap_or(i64::MAX))
            }
            "revision" => Ok(i64::try_from(context.revision).unwrap_or(i64::MAX)),
            "selection.time" => Ok(i64::from(context.selection.time.is_some())),
            "snap" => Ok(i64::from(context.snap_enabled)),
            "focus" => Err(WhenError::new("focus compares with a named value")),
            _ => Err(WhenError::new(format!("unknown field {path}"))),
        }
    }
}

enum Literal {
    Number(i64),
    Ident(String),
}

fn tokenize(expr: &str) -> Result<Vec<Token>, WhenError> {
    let mut tokens = Vec::new();
    let bytes = expr.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte.is_ascii_whitespace() {
            index += 1;
            continue;
        }
        if byte.is_ascii_alphabetic() || byte == b'_' {
            let start = index;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric()
                    || matches!(bytes[index], b'_' | b'.' | b'-'))
            {
                index += 1;
            }
            tokens.push(Token::Ident(expr[start..index].to_owned()));
            continue;
        }
        if byte.is_ascii_digit() {
            let start = index;
            while index < bytes.len() && bytes[index].is_ascii_digit() {
                index += 1;
            }
            let value = expr[start..index]
                .parse::<i64>()
                .map_err(|_| WhenError::new("number is out of range"))?;
            tokens.push(Token::Number(value));
            continue;
        }
        let two = bytes.get(index..index + 2);
        match two {
            Some(b"&&") => {
                tokens.push(Token::And);
                index += 2;
            }
            Some(b"||") => {
                tokens.push(Token::Or);
                index += 2;
            }
            Some(b"==") => {
                tokens.push(Token::Eq);
                index += 2;
            }
            Some(b"!=") => {
                tokens.push(Token::Ne);
                index += 2;
            }
            Some(b">=") => {
                tokens.push(Token::Ge);
                index += 2;
            }
            Some(b"<=") => {
                tokens.push(Token::Le);
                index += 2;
            }
            _ => {
                match byte {
                    b'(' => tokens.push(Token::LParen),
                    b')' => tokens.push(Token::RParen),
                    b'!' => tokens.push(Token::Not),
                    b'>' => tokens.push(Token::Gt),
                    b'<' => tokens.push(Token::Lt),
                    _ => {
                        return Err(WhenError::new(format!(
                            "unexpected character {:?}",
                            char::from(byte)
                        )));
                    }
                }
                index += 1;
            }
        }
    }
    if tokens.is_empty() {
        return Err(WhenError::new("empty expression"));
    }
    Ok(tokens)
}
