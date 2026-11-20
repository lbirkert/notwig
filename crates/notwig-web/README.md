# notwig-web

Browser entry point for Notwig.

## Purpose

`notwig-web` is a `cdylib` compiled to `wasm32-unknown-unknown`. It exports a
`wasm-bindgen` handle that starts an `eframe::WebRunner` on an HTML canvas and
delegates application behavior to the target-neutral `notwig-app` library.

The crate is empty on non-wasm targets so workspace checks do not try to start
a browser application.

## Owns

- The `WebApp` JavaScript-facing handle.
- Starting `eframe` with the browser renderer.
- Forwarding the user-gesture audio enable action to `notwig-app`.
- Exposing panic status to the page.

## Does not own

- UI, workspace, or DSP behavior (see `notwig-app` and lower crates).
- Project storage or browser file dialogs (see `notwig-app::web`).
- HTTP serving, caching, or deployment. The Pages workflow builds this crate
  and copies `web/index.html` beside the generated package.

## Build

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-pack --locked
wasm-pack build crates/notwig-web --target web --release \\
  --out-dir ../../target/web/pkg --out-name notwig_web
cp web/index.html web/styles.css target/web/
python3 -m http.server --directory target/web 8080
```
