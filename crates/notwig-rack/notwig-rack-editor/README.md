# notwig-rack-editor

Native plugin editor-window hosting for the `notwig-rack` framework.

The crate is format-agnostic. Callers load a `notwig_rack_core::Plugin<f32>`
through any format wrapper, then hand it to `EditorWindow::open` together with a
title, sample rate, and maximum block size. The window:

- activates the plugin with a stereo layout,
- embeds the plugin's `PluginEditor` view,
- pumps the editor idle callback and follows its reported size,
- maps raw parameter ids to plugin indices in both directions,
- saves and restores state blobs,
- exposes Space-key transport toggles, and
- captures the content view as PNG for diagnostics and parity tests.

On macOS the implementation uses AppKit. Other targets compile a stub whose
`open` returns an error.

This crate is re-exported by the `notwig-rack` umbrella crate behind its
`editor` feature. The Notwig app keeps its engine-process policy, descriptor
types, and monitor plumbing in `notwig-plugins`; this crate only owns the
native window and editor lifecycle.
