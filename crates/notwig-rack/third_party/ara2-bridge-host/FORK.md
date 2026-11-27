# Notwig fork notice

This directory vendors `ara2-bridge-host` 0.3.0 from the
[`ara2-bridge`](https://github.com/entrepeneur4lyf/ara2-bridge) project
(MIT OR Apache-2.0).

The code is unchanged except for two additive, public accessors required by
`notwig-rack-ara`:

- `DocumentSession::controller_ref()` exposes the opaque
  `ARADocumentControllerRef` so a companion API can bind a processor to the
  document before the extension controller is attached.
- `DocumentSession::audio_source_id(handle)` exposes the stable
  `AudioSourceId` token so a host audio-access provider can map a source
  handle to its audio buffer.

No upstream behavior is changed, and no other files are edited.
