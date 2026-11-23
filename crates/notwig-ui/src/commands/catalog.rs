use egui::Key;

use super::KeyChord;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ShortcutCommand {
    Quit,
    NewProject,
    OpenProject,
    Save,
    SaveAs,
    ImportAudio,
    AudioSettings,
    RenderMixdown,
    Play,
    Record,
    Stop,
    ToggleLoop,
    LoopSelection,
    Undo,
    Redo,
    SelectAll,
    Copy,
    Cut,
    Paste,
    Duplicate,
    Delete,
    GroupTracks,
    NewInstrumentTrack,
    NewAudioTrack,
    JoinClips,
    ToggleTrackActive,
    FitArrangement,
    OpenSettings,
    ToggleInfoPanel,
    ToggleDeviceEditor,
    ToggleClipEditor,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CommandCategory {
    Application,
    Transport,
    Edit,
    Arrangement,
    View,
}

impl CommandCategory {
    pub const ALL: [Self; 5] = [
        Self::Application,
        Self::Transport,
        Self::Edit,
        Self::Arrangement,
        Self::View,
    ];

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Application => "Application",
            Self::Transport => "Transport",
            Self::Edit => "Edit",
            Self::Arrangement => "Arrangement",
            Self::View => "View",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CommandSpec {
    pub command: ShortcutCommand,
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub category: CommandCategory,
    pub defaults: &'static [KeyChord],
}

pub const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        command: ShortcutCommand::Quit,
        id: "quit",
        label: "Quit Notwig",
        description: "Exit Notwig.",
        category: CommandCategory::Application,
        defaults: &[KeyChord::command(Key::Q)],
    },
    CommandSpec {
        command: ShortcutCommand::NewProject,
        id: "new_project",
        label: "New Project",
        description: "Create a new empty project.",
        category: CommandCategory::Application,
        defaults: &[KeyChord::command(Key::N)],
    },
    CommandSpec {
        command: ShortcutCommand::OpenProject,
        id: "open_project",
        label: "Open Project",
        description: "Open an existing project bundle.",
        category: CommandCategory::Application,
        defaults: &[KeyChord::command(Key::O)],
    },
    CommandSpec {
        command: ShortcutCommand::Save,
        id: "save",
        label: "Save",
        description: "Save the current project bundle.",
        category: CommandCategory::Application,
        defaults: &[KeyChord::command(Key::S)],
    },
    CommandSpec {
        command: ShortcutCommand::SaveAs,
        id: "save_as",
        label: "Save As",
        description: "Save the project to a new bundle.",
        category: CommandCategory::Application,
        defaults: &[KeyChord {
            key: Key::S,
            command: true,
            ctrl: false,
            alt: false,
            shift: true,
        }],
    },
    CommandSpec {
        command: ShortcutCommand::ImportAudio,
        id: "import_audio",
        label: "Import Audio",
        description: "Import an audio file into the project bundle.",
        category: CommandCategory::Application,
        defaults: &[],
    },
    CommandSpec {
        command: ShortcutCommand::AudioSettings,
        id: "audio_settings",
        label: "Audio Settings",
        description: "Open the audio device settings.",
        category: CommandCategory::Application,
        defaults: &[],
    },
    CommandSpec {
        command: ShortcutCommand::RenderMixdown,
        id: "render_mixdown",
        label: "Render Mixdown",
        description: "Render the arrangement or selection to a WAV file.",
        category: CommandCategory::Application,
        defaults: &[],
    },
    CommandSpec {
        command: ShortcutCommand::Play,
        id: "play",
        label: "Play / stop",
        description: "Play, or stop playback when already playing.",
        category: CommandCategory::Transport,
        defaults: &[KeyChord {
            key: Key::Space,
            command: false,
            ctrl: false,
            alt: false,
            shift: false,
        }],
    },
    CommandSpec {
        command: ShortcutCommand::Record,
        id: "record",
        label: "Record",
        description: "Start or stop recording on armed audio tracks.",
        category: CommandCategory::Transport,
        defaults: &[KeyChord::command(Key::R)],
    },
    CommandSpec {
        command: ShortcutCommand::Stop,
        id: "stop",
        label: "Stop",
        description: "Stop playback and return to the play start.",
        category: CommandCategory::Transport,
        defaults: &[KeyChord {
            key: Key::Space,
            command: false,
            ctrl: false,
            alt: false,
            shift: true,
        }],
    },
    CommandSpec {
        command: ShortcutCommand::ToggleLoop,
        id: "toggle_loop",
        label: "Toggle Loop",
        description: "Turn the transport loop on or off.",
        category: CommandCategory::Transport,
        defaults: &[],
    },
    CommandSpec {
        command: ShortcutCommand::LoopSelection,
        id: "loop_selection",
        label: "Loop selection / toggle loop",
        description: "Loop the selected time range or clip bounds, or toggle the loop.",
        category: CommandCategory::Transport,
        defaults: &[KeyChord::command(Key::L)],
    },
    CommandSpec {
        command: ShortcutCommand::Undo,
        id: "undo",
        label: "Undo",
        description: "Undo the last edit.",
        category: CommandCategory::Edit,
        defaults: &[KeyChord::command(Key::Z)],
    },
    CommandSpec {
        command: ShortcutCommand::Redo,
        id: "redo",
        label: "Redo",
        description: "Redo the last undone edit.",
        category: CommandCategory::Edit,
        defaults: &[KeyChord {
            key: Key::Z,
            command: true,
            ctrl: false,
            alt: false,
            shift: true,
        }],
    },
    CommandSpec {
        command: ShortcutCommand::SelectAll,
        id: "select_all",
        label: "Select All",
        description: "Select every object in the focused editor.",
        category: CommandCategory::Edit,
        defaults: &[KeyChord::command(Key::A)],
    },
    CommandSpec {
        command: ShortcutCommand::Copy,
        id: "copy",
        label: "Copy",
        description: "Copy selected clips or time range.",
        category: CommandCategory::Edit,
        defaults: &[KeyChord::command(Key::C)],
    },
    CommandSpec {
        command: ShortcutCommand::Cut,
        id: "cut",
        label: "Cut",
        description: "Cut selected clips or time range.",
        category: CommandCategory::Edit,
        defaults: &[KeyChord::command(Key::X)],
    },
    CommandSpec {
        command: ShortcutCommand::Paste,
        id: "paste",
        label: "Paste",
        description: "Paste the clipboard at the playhead or selection.",
        category: CommandCategory::Edit,
        defaults: &[KeyChord::command(Key::V)],
    },
    CommandSpec {
        command: ShortcutCommand::Duplicate,
        id: "duplicate",
        label: "Duplicate",
        description: "Duplicate selected clips, time range, or track.",
        category: CommandCategory::Edit,
        defaults: &[KeyChord::command(Key::D)],
    },
    CommandSpec {
        command: ShortcutCommand::Delete,
        id: "delete",
        label: "Delete",
        description: "Delete selected clips, time range, or tracks.",
        category: CommandCategory::Edit,
        defaults: &[
            KeyChord {
                key: Key::Delete,
                command: false,
                ctrl: false,
                alt: false,
                shift: false,
            },
            KeyChord {
                key: Key::Backspace,
                command: false,
                ctrl: false,
                alt: false,
                shift: false,
            },
        ],
    },
    CommandSpec {
        command: ShortcutCommand::NewInstrumentTrack,
        id: "new_instrument_track",
        label: "Add Instrument Track",
        description: "Create a new instrument track.",
        category: CommandCategory::Arrangement,
        defaults: &[KeyChord::command(Key::T)],
    },
    CommandSpec {
        command: ShortcutCommand::NewAudioTrack,
        id: "new_audio_track",
        label: "Add Audio Track",
        description: "Create a new audio track.",
        category: CommandCategory::Arrangement,
        defaults: &[KeyChord {
            key: Key::T,
            command: true,
            ctrl: false,
            alt: false,
            shift: true,
        }],
    },
    CommandSpec {
        command: ShortcutCommand::JoinClips,
        id: "join_clips",
        label: "Join Clips",
        description: "Join selected adjacent clips on the same lane.",
        category: CommandCategory::Edit,
        defaults: &[KeyChord::command(Key::J)],
    },
    CommandSpec {
        command: ShortcutCommand::GroupTracks,
        id: "group_tracks",
        label: "Group Tracks",
        description: "Group the selected tracks under a new group track.",
        category: CommandCategory::Arrangement,
        defaults: &[KeyChord::command(Key::G)],
    },
    CommandSpec {
        command: ShortcutCommand::ToggleTrackActive,
        id: "toggle_track_active",
        label: "Disable Track",
        description: "Toggle the active state of the selected tracks.",
        category: CommandCategory::Arrangement,
        defaults: &[KeyChord {
            key: Key::A,
            command: false,
            ctrl: false,
            alt: true,
            shift: false,
        }],
    },
    CommandSpec {
        command: ShortcutCommand::FitArrangement,
        id: "fit_arrangement",
        label: "Fit Arrangement",
        description: "Zoom the timeline to fit the arrangement.",
        category: CommandCategory::Arrangement,
        defaults: &[KeyChord::command(Key::Num0)],
    },
    CommandSpec {
        command: ShortcutCommand::OpenSettings,
        id: "open_settings",
        label: "Open Settings",
        description: "Open the settings window.",
        category: CommandCategory::View,
        defaults: &[KeyChord::command(Key::Comma)],
    },
    CommandSpec {
        command: ShortcutCommand::ToggleInfoPanel,
        id: "toggle_info_panel",
        label: "Toggle Info Panel",
        description: "Show or hide the selection info panel.",
        category: CommandCategory::View,
        defaults: &[],
    },
    CommandSpec {
        command: ShortcutCommand::ToggleDeviceEditor,
        id: "toggle_device_editor",
        label: "Toggle Device Editor",
        description: "Open or close the device editor.",
        category: CommandCategory::View,
        defaults: &[],
    },
    CommandSpec {
        command: ShortcutCommand::ToggleClipEditor,
        id: "toggle_clip_editor",
        label: "Toggle Clip Editor",
        description: "Open or close the clip editor.",
        category: CommandCategory::View,
        defaults: &[],
    },
    CommandSpec {
        command: ShortcutCommand::Cancel,
        id: "cancel",
        label: "Cancel / close editor",
        description: "Close the focused editor or clear the selection.",
        category: CommandCategory::View,
        defaults: &[KeyChord {
            key: Key::Escape,
            command: false,
            ctrl: false,
            alt: false,
            shift: false,
        }],
    },
];

impl ShortcutCommand {
    #[must_use]
    pub fn spec(self) -> &'static CommandSpec {
        COMMANDS
            .iter()
            .find(|spec| spec.command == self)
            .expect("every ShortcutCommand has a command spec")
    }

    #[must_use]
    pub fn id(self) -> &'static str {
        self.spec().id
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        self.spec().label
    }

    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        COMMANDS
            .iter()
            .find(|spec| spec.id == id)
            .map(|spec| spec.command)
    }
}
