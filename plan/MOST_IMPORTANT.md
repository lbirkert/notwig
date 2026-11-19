1. Export formats	📋 Planned	⭐⭐⭐⭐	AIFF, FLAC, MP3, OGG, bit depth, dithering, normalization, and stem export.
2. Render progress and cancel	📋 Planned	⭐⭐⭐	GUI render blocks the UI thread today.
Render progress should be displayed using the playhead. Put it where the render currently is.
Also use the new notification system (below) to create a notification with a status bar (in percent) and a Cancel button.
3. Implement proper notification system
Notifications should pop up as boxes in the top right. This should be used for the most important stuff like audio engine crashes, errors, tasks (like rendering audio or analyzing audio).
4. Improve performance of dragging audio into the application.
Bitwig has quick length analysis that allows it to render a preview in milliseconds. If we drag a normal MP3 (3mins) it takes about 10 seconds to render a preview. WAV preview takes about 3 seconds. Audio analyis also is slow and blocks the UI Thread. Bitwig displays a Analyzing audio status and offloads analysis to a thread.
5. Metronome	📋 Planned	⭐⭐⭐⭐	
6. Plugin state is not persisted	⭐⭐⭐⭐⭐	A reopened project cannot restore plugin settings or duplicate a plugin faithfully.
Persist plugin state in project.
7. Plugin latency is unavailable	⭐⭐⭐⭐⭐	Sends, nested groups, bypassed devices, and track routing are compensated now. Hosted plugin latency cannot be queried through truce-rack 1.1.5, so latent plugins will still drift.
Clone the repository into crates/notwig-truce-rack and make the required changes to query plugin latency. Use this new library as a drop in replacement of truce-rack.
