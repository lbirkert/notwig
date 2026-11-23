use egui::{Key, Modifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct KeyChord {
    pub key: Key,
    pub command: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

impl KeyChord {
    #[must_use]
    pub const fn command(key: Key) -> Self {
        Self {
            key,
            command: true,
            ctrl: false,
            alt: false,
            shift: false,
        }
    }

    #[must_use]
    pub fn from_modifiers(key: Key, modifiers: Modifiers) -> Self {
        Self {
            key,
            command: modifiers.command,
            ctrl: modifiers.ctrl && !modifiers.command,
            alt: modifiers.alt,
            shift: modifiers.shift,
        }
    }

    #[must_use]
    pub fn is_pressed(self, input: &egui::InputState) -> bool {
        input.key_pressed(self.key)
            && input.modifiers.command == self.command
            && (input.modifiers.ctrl && !input.modifiers.command) == self.ctrl
            && input.modifiers.alt == self.alt
            && input.modifiers.shift == self.shift
    }

    #[must_use]
    pub fn label(self) -> String {
        let mut parts = Vec::new();
        if self.command {
            parts.push("Cmd".to_owned());
        }
        if self.ctrl {
            parts.push("Ctrl".to_owned());
        }
        if self.alt {
            parts.push("Alt".to_owned());
        }
        if self.shift {
            parts.push("Shift".to_owned());
        }
        parts.push(key_label(self.key).to_owned());
        parts.join("+")
    }

    #[must_use]
    pub fn config_string(self) -> String {
        let mut parts = Vec::new();
        if self.command {
            parts.push("Cmd".to_owned());
        }
        if self.ctrl {
            parts.push("Ctrl".to_owned());
        }
        if self.alt {
            parts.push("Alt".to_owned());
        }
        if self.shift {
            parts.push("Shift".to_owned());
        }
        parts.push(self.key.name().to_owned());
        parts.join("+")
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        let mut command = false;
        let mut ctrl = false;
        let mut alt = false;
        let mut shift = false;
        let mut key = None;
        for part in value
            .split('+')
            .map(str::trim)
            .filter(|part| !part.is_empty())
        {
            match part.to_ascii_lowercase().as_str() {
                "cmd" | "command" => command = true,
                "ctrl" | "control" => ctrl = true,
                "alt" | "option" => alt = true,
                "shift" => shift = true,
                other => {
                    if key.is_some() {
                        return None;
                    }
                    let found = Key::ALL.iter().copied().find(|candidate| {
                        candidate.name().eq_ignore_ascii_case(other)
                            || key_label(*candidate).eq_ignore_ascii_case(other)
                    })?;
                    key = Some(found);
                }
            }
        }
        Some(Self {
            key: key?,
            command,
            ctrl,
            alt,
            shift,
        })
    }
}

fn key_label(key: Key) -> &'static str {
    match key {
        Key::Comma => ",",
        Key::Period => ".",
        Key::Slash => "/",
        Key::Backslash => "\\",
        Key::Minus => "-",
        Key::Equals => "=",
        Key::Plus => "+",
        Key::Semicolon => ";",
        Key::OpenBracket => "[",
        Key::CloseBracket => "]",
        Key::Backtick => "`",
        Key::Quote => "'",
        Key::Escape => "Esc",
        other => other.name(),
    }
}
