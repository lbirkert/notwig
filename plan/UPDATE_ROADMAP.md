Lock-free engine command queue	📋 Planned	⭐⭐⭐⭐⭐	 -> not necessary since we swap engine_data
Engine updates are synchronous	⭐⭐⭐⭐⭐	The UI and audio callback share a mutex. The callback can fail to lock and output silence. -> stale since engine update
DSP load window	🧪 Partial	⭐⭐	Shows callback history, average and max load, deadline, jitter, and output latency. The window render is tested. -> this should be implemented

Device A/B compare	📋 Planned	⭐⭐⭐ -> not planned, can just disable plugin for A/B
Stock instruments	📋 Planned	⭐⭐⭐⭐	Start with one simple synth and one sampler. -> 2 Stars
Stock effects	📋 Planned	⭐⭐⭐⭐	Delay, reverb, compressor, limiter, filter, saturation, and chorus. -> 3 stars
Parameter modulation	📋 Planned	⭐⭐⭐⭐	Modulation and macro flags exist as reserved metadata.
Macro controls	📋 Planned	⭐⭐⭐⭐	
MIDI learn	📋 Planned	⭐⭐⭐	
Control surface support	📋 Planned	⭐⭐⭐	
Sample-accurate parameter ramps	📋 Planned	⭐⭐⭐⭐	Parameter changes apply at block boundaries. RampSpec is carried and ignored.
Modulator devices	📋 Planned	⭐⭐⭐	Basic LFO and envelope modulators. Not The Grid. -> for now, above not planned 
MIDI output routing	📋 Planned	⭐⭐⭐⭐ -> not planned

Drum editor and pad view	📋 Planned	⭐⭐⭐	
Step sequencer	📋 Planned	⭐⭐ -> not planned

Dedicated mixer view	📋 Planned	⭐⭐⭐⭐⭐	Mixer controls live in track headers and the inspector.
Channel strip processing	📋 Planned	⭐⭐⭐⭐	EQ and dynamics on every channel.
VCA groups	📋 Planned	⭐⭐⭐
Track templates	📋 Planned	⭐⭐ -> not planned

Bus and return tracks	📋 Planned	⭐⭐⭐⭐⭐	Bus FX exists as a track kind. There is no return or send-return pairing. -> not planned

Sends	🧪 Partial	⭐⭐⭐⭐⭐	Engine routes pre and post fader. No UI. Pre-fader is currently pre-FX too. -> not planned

External clock and sync	📋 Planned	⭐⭐ -> not planned

Playhead locate	🧪 Partial	⭐⭐⭐⭐	Ruler sets the play start. There is no scrub or seek gesture. -> not planned
