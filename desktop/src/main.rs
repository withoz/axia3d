//! AxiA 3D as a desktop application.
//!
//! The whole engine already runs in a webview — Rust compiled to WASM, driven
//! by TypeScript, drawn by Three.js. This binary supplies the window that
//! webview lives in and nothing else: there is no second copy of the kernel
//! here, and no native code path that could disagree with the browser one.
//!
//! ⚠ It exists because of one thing the browser build cannot do offline. The
//! engine's WASM is fetched (`new URL('axia_wasm_bg.wasm', import.meta.url)`),
//! and Chromium refuses `fetch` on `file://`, so double-clicking the built
//! `index.html` gives a blank window. Tauri serves the bundle over its own
//! protocol, so the fetch succeeds and the app starts with no server to run.

// Release builds get the Windows subsystem so launching the exe does not also
// open a console window behind it. Debug keeps the console for `println!`.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("AxiA 3D failed to start");
}
