# Logging and Diagnostics

Notwig initializes `tracing` before parsing the CLI and keeps application,
engine, and helper-process logs separate. Logs are written both to a
timestamped file and to the console. The console is intentionally quiet; the
file is the diagnostic record.

Related: [`Application Lifecycle`](18_app_lifecycle.md),
[`Engine Process and Supervision`](15_engine_process_and_supervision.md), and
[`Plugin Hosting`](16_plugin_hosting.md).

## 1. Log files

Native builds write files into the Notwig app directory:

| Platform | Directory |
| --- | --- |
| macOS | `~/Library/Application Support/notwig/logs/` |
| Linux | `~/.config/notwig/logs/` |

Each process run creates one file:

| Process | File pattern | Contents |
| --- | --- | --- |
| App / CLI | `app-YYYYMMDD-HHMMSSZ-<pid>.log` | UI, workspace, audio device, extensions, project, render, and plugin-catalog lifecycle |
| `engine-host` | `engine-YYYYMMDD-HHMMSSZ-<pid>.log` | Engine child startup, device opening, runtime builds, editor hosting, recording, and child errors |

The app directory is also the root for `audio.json`, `extensions.json`,
`extensions/`, `projects.json`, and `shortcuts.json`. The disposable plugin scan
cache lives in the platform cache directory instead
(`~/Library/Caches/notwig/` on macOS, `~/.cache/notwig/` on Linux).

Browser builds store the same structure in OPFS. The app log is
`notwig/logs/app-<timestamp>.log`. OPFS access is asynchronous, so browser
logging formats synchronously into a bounded buffer and the frame loop flushes
it with `logging::flush_web` in a local task.

## 2. Levels and filters

| Destination | Default filter | Environment override |
| --- | --- | --- |
| Native console (`stderr`) | `info` | `RUST_LOG` |
| Native app/engine file | `debug` | `NOTWIG_LOG` |
| Browser console | `info` | none |
| Browser OPFS file | `debug` | none |

`info` is reserved for diagnostics a user can act on or report:

- application startup and the active log path;
- opening or saving a project, with elapsed time;
- scanning audio plugins, with count and elapsed time;
- opening an audio device, including driver, device, rate, and channels;
- the supervised engine process becoming ready;
- completed offline WAV rendering.

Most internal events are `debug`: engine build requests and installs, command
application, undo/redo, copy/cut/paste, extension and plugin metadata details,
and helper-process output. Hot paths and high-frequency parameter changes must
not log at `info`. Use `NOTWIG_LOG=trace` temporarily when a debug-only record is
not detailed enough.

Internal helper subprocesses (`scan-plugins`, `load-plugin`, `probe-plugin`)
write only to `stderr` with ANSI disabled and no file of their own. Their parent
captures that stream and forwards each non-empty line through `tracing` at
`debug`, so plugin scan and probe diagnostics land in the owning app log instead
of leaking to the terminal. The vendored rack scanners also use `tracing` for
per-plugin skip messages.

## 3. Real-time rule

The audio callback must never format, lock, allocate, or write a log line. The
engine child logs only from its IPC, worker, loader, and editor threads, never
from the CPAL callback. Native file logging formats each event synchronously, then sends the complete
event to a dedicated log-writer thread so a slow disk cannot stall a producer
thread. Browser logs
are bounded to 2 MiB between flushes; if OPFS cannot keep up, lines are dropped
rather than growing memory without limit.
