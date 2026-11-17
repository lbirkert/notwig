# Tracks & Routing (`notwig-core`)

## 1. Track Types (v0)
`TrackKind = Audio | Instrument | Group | BusFx | Master`
- **Audio:** hosts audio clips; audio device chain. Playback only in v0 (no input/recording).
- **Instrument:** hosts MIDI clips (notes only); chain contains zero or one hosted instrument device (VST3/AU, VST2 fallback) that consumes note events and produces audio; NoteFx may precede it, AudioFx may follow. Zero instruments is valid and silent.
- **Group:** folder/summing track with children; routes/sums child outputs; has own mixer and device chain. No clips or lane.
- **BusFx:** auxiliary bus for sends/returns/submix routing. No clips, lane, or children.
- **Master:** unique project sink/root output chain. No clips or lane.
- **Lanes/clips** exist only on `Audio` and `Instrument` tracks in v0.

## 2. Track Entity
`Track = {
  id: Id<Track>,
  kind: TrackKind,
  name: String,
  color: Option<Color>,
  parent: Option<Id<Track>>,
  mixer: MixerState,
  io: TrackIO,
  mainChain: Id<DeviceChain>,
  sends: Vec<Send>
}`

`MixerState = { volumeDb: Db, pan: Pan, mute: bool, solo: bool }`
`Send = { id: Id<Send>, target: Id<Track>, amountDb: Db, preFader: bool, enabled: bool }`
`TrackIO = { audioIn: Option<AudioEndpointRef> (reserved), midiIn: Option<MidiEndpointRef> (reserved), audioOut: TrackOutput }`
- `arm`/`monitor` and input endpoints are reserved for future recording/live input and are not part of v0 state.
- `mainChain` is the track's single ordered device chain. Instrument tracks contain zero or one `Instrument` device; when present, NoteFx are ordered before it and AudioFx after it. Zero instruments means a silent (but valid) Instrument track.

## 3. Structural Constraints
- Exactly one `Master` track.
- `trackOrder` is an ordered forest of all track IDs:
  - each `Id<Track>` appears exactly once
  - `parent == Some(p)` iff node is child of `p`
  - only `Group` may have children in v0
  - no cycles
- `Master` has no parent, is the final sink, has no lane/clips, and is not a valid parent.
- Only `Audio` and `Instrument` tracks own lanes/clips; Group/BusFx/Master reject clip operations.
- `DeleteTrack` rejects `Master`; descendant tracks (children), their lanes/clips, and their devices are deleted recursively. Sends targeting deleted tracks are removed, and the engine plan is rebuilt. Deleted IDs are never reused.
- Sends/routes must reference existing tracks; moving a track that would create a cycle or invalid parent is rejected.
- Device placement validation: `Instrument` devices only on Instrument tracks (at most one), `NoteFx` only on Instrument tracks (before the instrument when present), and `AudioFx` on any track chain; `Container` follows the same kind rules for its children.

## 4. Routing Model
`RoutingGraph = { connections: Vec<Connection> }` (derived/explicit as represented) with typed ports/signals:
- `Audio(ch)`, `Midi`, `NoteEvent` (internal), optional sidechain aux inputs reserved.
- Default: track main output -> parent (or `Master` if top-level) unless explicitly overridden by a valid connection.
- Sends: tap from track signal path per `preFader`; target must be `BusFx|Group|Master` (v0 rule) and type-compatible.
- Synchronous audio routing MUST be acyclic in v0. No feedback loops.

Validation rejects: missing endpoints, type mismatch, cycles, illegal targets, or multiple masters.

## 5. Mixer & Projection Semantics
- Mixer/Arranger/Device UI operate on same track IDs and `S_doc` state.
- `S_rt` provides meters/peaks/status per track; mixer parameter edits mutate `S_doc` and emit `EngineMsg`.
- v0 deterministic policy:
  - `mute=true` silences the track output after its chain/fader, affecting the main route and sends alike.
  - `solo`: if any track is soloed, non-soloed audio sources are silenced; the solo set is expanded to the ancestor path required to reach `Master`, and to Group children where the Group itself is soloed.
  - Exact audition dependency resolution is centralized and must be consistent across UI metering and render; v0 policy is deterministic for a given `S_doc`. BusFx participation is limited to soloed buses or audible sends feeding them.
  - Pan law (v0 provisional): constant-power stereo pan, -3 dB at center; shared deterministically by engine and UI.
  - Default send is post-fader; `preFader=true` taps before the fader but after the device chain.
