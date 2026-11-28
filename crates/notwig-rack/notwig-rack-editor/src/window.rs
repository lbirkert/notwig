#[cfg(target_os = "macos")]
mod imp {
    use std::collections::BTreeMap;
    use std::ptr;
    use std::sync::atomic::Ordering;
    use std::time::{Duration, Instant};

    use notwig_rack_core::buffer::BusRange;
    use notwig_rack_core::{
        AudioBuffer, BusLayout, Error, EventList, Plugin, ProcessContext, ProcessStatus, Result,
        WindowHandle,
    };
    use objc2::msg_send;
    use objc2::runtime::AnyObject;

    use crate::appkit::{capture_window_png, create_window, lock_window_size, release_window};
    use crate::space_monitor::{
        PLAYBACK_TOGGLE_PENDING, register_plugin_window, unregister_plugin_window,
    };

    /// A native plugin editor window and its host-side editor plugin instance.
    ///
    /// The window owns the plugin instance for its whole lifetime. Closing the
    /// window (or dropping it) closes the plugin editor, unregisters the key
    /// monitor, and releases the native window.
    pub struct EditorWindow {
        plugin: Box<dyn Plugin<f32>>,
        window: *mut AnyObject,
        open: bool,
        params: BTreeMap<u32, usize>,
        last_values: BTreeMap<u32, f64>,
        last_poll: Instant,
        window_size: (u32, u32),
        sample_rate: f64,
        max_block_size: usize,
        scratch_input: [Vec<f32>; 2],
        scratch_output: [Vec<f32>; 2],
    }

    impl EditorWindow {
        /// Open a native editor window for an already-loaded plugin.
        ///
        /// `sample_rate` and `max_block_size` are used to activate the plugin
        /// for the editor's monitor processing. The window starts at a
        /// host-chosen size and immediately snaps to the editor's reported
        /// size once the editor is open.
        ///
        /// # Errors
        /// Returns an error when the native window cannot be created, the
        /// plugin has no editor, or the editor rejects the parent view.
        pub fn open(
            mut plugin: Box<dyn Plugin<f32>>,
            title: &str,
            sample_rate: f64,
            max_block_size: usize,
        ) -> Result<Self> {
            let sample_rate = sample_rate.max(1.0);
            let max_block_size = max_block_size.max(1);
            let layout = editor_layout(plugin.as_ref());
            let _ = plugin.activate(layout, sample_rate, max_block_size);

            let mut params = BTreeMap::new();
            let mut last_values = BTreeMap::new();
            for index in 0..plugin.parameter_count() {
                if let Ok(info) = plugin.parameter_info(index) {
                    params.insert(info.id, index);
                    if let Ok(value) = plugin.parameter_value(index) {
                        last_values.insert(info.id, value);
                    }
                }
            }

            let window = unsafe { create_window(title, 760.0, 560.0) }?;
            let content: *mut AnyObject = unsafe { msg_send![window, contentView] };
            if content.is_null() {
                unsafe {
                    release_window(window);
                }
                return Err(Error::Other("plugin window has no content view".into()));
            }
            let Some(editor) = plugin.editor() else {
                unsafe {
                    release_window(window);
                }
                return Err(Error::Other("plugin has no editor".into()));
            };
            if let Err(err) = editor.open(WindowHandle::NSView(content.cast()), 1.0) {
                editor.close();
                unsafe {
                    release_window(window);
                }
                return Err(err);
            }
            let (plugin_size, resizable) = (
                editor.size().filter(|(w, h)| *w > 0 && *h > 0),
                editor.is_resizable(),
            );
            editor.show();
            let window_size = plugin_size.unwrap_or((760, 560));
            unsafe {
                lock_window_size(window, window_size, resizable);
                let _: () = msg_send![window, center];
                let _: () = msg_send![window, makeKeyAndOrderFront: ptr::null_mut::<AnyObject>()];
            }
            register_plugin_window(window);
            Ok(Self {
                plugin,
                window,
                open: true,
                params,
                last_values,
                last_poll: Instant::now(),
                window_size,
                sample_rate,
                max_block_size,
                scratch_input: std::array::from_fn(|_| vec![0.0; max_block_size]),
                scratch_output: std::array::from_fn(|_| vec![0.0; max_block_size]),
            })
        }

        /// Bring the window to the front. The plugin's editor keeps focus and
        /// receives the keyboard.
        pub fn focus(&self) {
            if self.open {
                unsafe {
                    let _: () = msg_send![
                        self.window,
                        makeKeyAndOrderFront: ptr::null_mut::<AnyObject>()
                    ];
                }
            }
        }

        /// Drive one UI frame: notice window close, call the plugin idle hook,
        /// and keep the outer window matched to the editor's reported size.
        pub fn pump(&mut self) {
            if !self.open {
                return;
            }
            let visible: bool = unsafe { msg_send![self.window, isVisible] };
            if !visible {
                self.close();
                return;
            }
            let (plugin_size, resizable) = if let Some(editor) = self.plugin.editor() {
                editor.on_idle();
                (
                    editor.size().filter(|(w, h)| *w > 0 && *h > 0),
                    editor.is_resizable(),
                )
            } else {
                (None, false)
            };
            if let Some(size) = plugin_size
                && size != self.window_size
            {
                unsafe {
                    lock_window_size(self.window, size, resizable);
                }
                self.window_size = size;
            }
        }

        /// `true` while the native window is open.
        #[must_use]
        pub fn is_open(&self) -> bool {
            self.open
        }

        /// Current logical content size of the hosted editor window.
        #[must_use]
        pub fn size(&self) -> (u32, u32) {
            self.window_size
        }

        /// Returns and clears a playback-toggle request recorded while a plugin
        /// window was key.
        pub fn take_playback_toggle(&mut self) -> bool {
            PLAYBACK_TOGGLE_PENDING.swap(false, Ordering::AcqRel)
        }

        /// Set a plugin parameter by its raw plugin id.
        pub fn set_parameter(&mut self, id: u32, value: f64) {
            if let Some(index) = self.params.get(&id).copied() {
                let _ = self.plugin.set_parameter(index, value);
                self.last_values.insert(id, value);
            }
        }

        /// Snapshot the editor plugin's state.
        ///
        /// # Errors
        /// Returns the format wrapper's state-save error.
        /// # Errors
        /// Always returns [`Error::Other`] on non-macOS platforms.
        pub fn save_state(&self) -> Result<Vec<u8>> {
            self.plugin.save_state()
        }

        /// Restore a state blob into the editor plugin.
        ///
        /// # Errors
        /// Returns the format wrapper's state-load error.
        pub fn load_state(&mut self, bytes: &[u8]) -> Result<()> {
            self.plugin.load_state(bytes)
        }

        /// Poll parameters changed by the plugin editor.
        ///
        /// Polling is rate-limited to roughly 20 Hz; callers can call this
        /// every UI frame.
        pub fn poll_changes(&mut self) -> Vec<(u32, f64)> {
            if self.last_poll.elapsed() < Duration::from_millis(50) {
                return Vec::new();
            }
            self.last_poll = Instant::now();
            let mut changes = Vec::new();
            for (id, index) in &self.params {
                let Ok(value) = self.plugin.parameter_value(*index) else {
                    continue;
                };
                let differs = self
                    .last_values
                    .get(id)
                    .is_none_or(|previous| (previous - value).abs() > 1.0e-4);
                if differs {
                    changes.push((*id, value));
                    self.last_values.insert(*id, value);
                }
            }
            changes
        }

        /// Process one interleaved stereo block through the editor plugin.
        ///
        /// Used by hosts to feed a monitor tap into the editor instance so
        /// plugin meters and visualizers stay live. `input` must contain whole
        /// stereo frames (an even number of samples). No output is returned.
        ///
        /// # Errors
        /// Returns an error when the block exceeds the activated maximum block
        /// size or when the plugin reports a process error.
        pub fn process_interleaved(&mut self, input: &[f32]) -> Result<()> {
            let frames = input.len() / 2;
            if frames == 0 {
                return Ok(());
            }
            if frames > self.max_block_size {
                return Err(Error::Other(format!(
                    "editor monitor block of {frames} frames exceeds max block size {}",
                    self.max_block_size
                )));
            }
            for channel in 0..2 {
                let scratch = &mut self.scratch_input[channel];
                scratch.fill(0.0);
                for frame in 0..frames {
                    scratch[frame] = input[frame * 2 + channel];
                }
                self.scratch_output[channel][..frames].fill(0.0);
            }

            let input_refs: [&[f32]; 2] = [
                &self.scratch_input[0][..frames],
                &self.scratch_input[1][..frames],
            ];
            let (left, right) = self.scratch_output.split_at_mut(1);
            let mut output_refs: [&mut [f32]; 2] =
                [&mut left[0][..frames], &mut right[0][..frames]];
            let buses = [BusRange::new(0, 2)];
            let mut buffer =
                AudioBuffer::new(&input_refs, &mut output_refs, frames, &buses, &buses);
            let events = EventList::new();
            let mut output_events = EventList::new();
            let mut context = ProcessContext {
                sample_rate: self.sample_rate,
                max_block_size: self.max_block_size,
                transport: None,
                output_events: &mut output_events,
            };
            match self.plugin.process(&mut buffer, &events, &mut context)? {
                ProcessStatus::Error => Err(Error::Other("editor plugin process failed".into())),
                ProcessStatus::Continue | ProcessStatus::Sleep | ProcessStatus::Tail { .. } => {
                    Ok(())
                }
            }
        }

        /// Capture the plugin content view as PNG bytes.
        ///
        /// The image covers the window's content area (excluding the title
        /// bar). It is intended for diagnostics and visual parity tests; some
        /// GPU-backed plugin editors may not render into an offscreen bitmap.
        ///
        /// # Errors
        /// Returns an error when the window is closed or the view cannot
        /// render/encode a PNG.
        /// # Errors
        /// Always returns [`Error::Other`] on non-macOS platforms.
        pub fn capture_png(&self) -> Result<Vec<u8>> {
            if !self.open {
                return Err(Error::Other("plugin editor window is closed".into()));
            }
            unsafe { capture_window_png(self.window) }
        }

        fn close(&mut self) {
            if !self.open {
                return;
            }
            if let Some(editor) = self.plugin.editor() {
                editor.close();
            }
            unregister_plugin_window(self.window);
            unsafe {
                let _: () = msg_send![self.window, orderOut: ptr::null_mut::<AnyObject>()];
                release_window(self.window);
            }
            self.window = ptr::null_mut();
            self.open = false;
        }
    }

    impl Drop for EditorWindow {
        fn drop(&mut self) {
            self.close();
        }
    }

    fn editor_layout(plugin: &dyn Plugin<f32>) -> BusLayout {
        plugin
            .supported_layouts()
            .iter()
            .find(|layout| {
                layout.total_input_channels() == 2 && layout.total_output_channels() == 2
            })
            .cloned()
            .unwrap_or_else(BusLayout::stereo)
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use notwig_rack_core::{Error, Plugin, Result};

    /// Platform stub for native plugin editor windows.
    pub struct EditorWindow;

    impl EditorWindow {
        /// Native editor windows are unavailable on this target.
        ///
        /// # Errors
        /// Always returns [`Error::Other`] on non-macOS platforms.
        pub fn open(
            _plugin: Box<dyn Plugin<f32>>,
            _title: &str,
            _sample_rate: f64,
            _max_block_size: usize,
        ) -> Result<Self> {
            Err(Error::Other(
                "native plugin editor windows are unavailable on this platform".into(),
            ))
        }

        pub fn focus(&self) {}

        pub fn pump(&mut self) {}

        #[must_use]
        pub fn is_open(&self) -> bool {
            false
        }

        #[must_use]
        pub fn size(&self) -> (u32, u32) {
            (0, 0)
        }

        pub fn take_playback_toggle(&mut self) -> bool {
            false
        }

        pub fn set_parameter(&mut self, _id: u32, _value: f64) {}

        /// # Errors
        /// Always returns [`Error::Other`] on non-macOS platforms.
        pub fn save_state(&self) -> Result<Vec<u8>> {
            Err(Error::Other(
                "native plugin editor windows are unavailable on this platform".into(),
            ))
        }

        /// # Errors
        /// Always returns [`Error::Other`] on non-macOS platforms.
        pub fn load_state(&mut self, _bytes: &[u8]) -> Result<()> {
            Err(Error::Other(
                "native plugin editor windows are unavailable on this platform".into(),
            ))
        }

        pub fn poll_changes(&mut self) -> Vec<(u32, f64)> {
            Vec::new()
        }

        /// # Errors
        /// Always returns [`Error::Other`] on non-macOS platforms.
        pub fn process_interleaved(&mut self, _input: &[f32]) -> Result<()> {
            Err(Error::Other(
                "native plugin editor windows are unavailable on this platform".into(),
            ))
        }

        /// # Errors
        /// Always returns [`Error::Other`] on non-macOS platforms.
        pub fn capture_png(&self) -> Result<Vec<u8>> {
            Err(Error::Other(
                "native plugin editor windows are unavailable on this platform".into(),
            ))
        }
    }

    pub fn prepare_window_host() {}

    pub fn pump_window_host() {}
}

pub use imp::*;
