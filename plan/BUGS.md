- "Import audio before creating an audio clip" when double clicking an audio lane to create an empty audio clip.
- Make background of grid lighter.
- Make grid snapping less granular globally (1/16 -> 1/8 snapping).
- Newly created clips need to be selected (via Add Track or via CMD + (Shift) + T or CMD + G)
- CMD + L does not work for object selection (should take min/max object bounds)
- For audio editor, dragging on the audo event outside of the events audio handle should allow time selection. Same for arranger.
- Resizing an audio event leftwards at the beginning of its source cannot add silence yet. `AudioSlice` has no silent-prefix field, so the gesture stops at the source boundary. Resizing an audio clip leftwards does add leading silence.
- Device explorer does not show whether plugin is VST3, AU, or other
- Some plugin UIs have problems (LALA, Distox crashed, ...)
- Crash recovery should show unsaved projects when starting the app instead of just when loading a saved project.
- Time selection needs to be extendable horizontally
- Time selection markers should only extend over the lanes that are actually selected like in Bitwig
- Analyzing audio notification should have a progress bar (intermediate if no actual progress is available) and close as soon as the analysis is finished.
- Explore ways of running the audio analysis on the GPU for better MP3/WAV decoding performance.
- Make https://notwig.lbirkert.com/extensions.json the default extension index.
- Engine loop end changes bar (keeps timing I think) when tempo is changed while playing.

- Add draw loop functionality.

- Add proper tooltip system that displays the current available mouse actions (+ modifiers)

- Add a seperate Encoding Audio progress notification after rendering has finished.
--> should be a horizontal bar and not a circle spinner.
- Remove weird black padding for audio and midi editor.
- When having a plugin window opened and pressing space playback pause/stop works but macos plays a weird bell sound like the key could not have been handled.
- creating a note needs to update the time selection to note min/max
- Join does not work for time selection
- Grid snapping needs major rework
- We should list the editing operations in a matrix
- CMD+A select does not work
- In MIDI-Editor slicing multiple notes does not work (also for other editors)
- Title 

- Moving/adding audio track causes audio to not play :()
- Moving/adding midi track causes audio to not play :()
- Same for saving
- Same for engaging/disengaging metronome


- Instrument tracks should take the name of their instruments if no name is defined
- When zoom is too small, clips disapear
- Playing midi clip using clicking should not play so long for long clicks.
- Add system out device
- Detect when a device is unplugged/replugged.
- Add rightclick middle (tempo etc.) context menu that allows Stopping or Restarting audio engine.
- Fix extensions for browser.
- Web export does not yield a proper WAV
