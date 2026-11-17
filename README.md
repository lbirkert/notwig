# notwig

Notwig is an opensource clean-room reimplementation of Bitwig.

## Paradigms

- Everything must be extensible.
- Embrace simplicity wherever possible.
- Focus on a few high quality features.
- Stable DSP is the main focus.

## Inspiration & Dependencies

Reference projects and candidate dependencies (licenses/APIs reviewed before adoption; clean-room with respect to Bitwig/OpenLive):

- [truce-audio/truce-rack](https://github.com/truce-audio/truce-rack) — VST3, AU, and CLAP plugin loading; candidate dependency for Notwig's host adapters.
- [hikarucorporation/hikaru-openlive](https://github.com/hikarucorporation/hikaru-openlive) — Bitwig-inspired UI/workflow; Notwig targets a smaller, higher-quality feature subset.

## License

Notwig is licensed under the [Mozilla Public License](LICENSE).
