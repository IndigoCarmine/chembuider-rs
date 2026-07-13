# Building `editor` for the web

The `editor` example also targets `wasm32-unknown-unknown`, using eframe's `wgpu` backend
(WebGPU, auto-falling back to WebGL2 in browsers without WebGPU support). The `glow` backend
used for the native build isn't available on wasm32, since it depends on native-only
windowing (`glutin`).

## One-time setup

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.125
```

`wasm-bindgen-cli`'s version must match the `wasm-bindgen` crate version resolved in
`Cargo.lock` exactly (check with `grep -A1 'name = "wasm-bindgen"' Cargo.lock`), or the CLI
will refuse to process the build output.

## Build

```sh
cargo build --target wasm32-unknown-unknown --example editor --release
wasm-bindgen --target web --no-typescript \
    --out-dir examples/editor_web \
    --out-name editor \
    target/wasm32-unknown-unknown/release/examples/editor.wasm
```

This writes `editor.js` and `editor_bg.wasm` next to `index.html` in this directory.

## Serve

ES modules require `http(s)://`, not `file://`. From this directory:

```sh
python -m http.server 8080
```

Then open <http://localhost:8080>.

## Known limitations on the web

`Config::save()`, fragment saving, and clipboard paste all go through `std::fs`, which has
no filesystem to talk to in a browser — those buttons will show a "failed" status message
instead of erroring at compile time. `Save MOL2` instead triggers a browser download (via a
generated `Blob` + `<a download>`) rather than a native save dialog.
