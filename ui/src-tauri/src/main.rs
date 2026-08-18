// Prevents additional console window on Windows in release, DO NOT REMOVE!!
// `desktop:standalone` enables the task-057 diagnostic feature temporarily so
// Windows can show the batch/OLE trace in its invoking terminal. Packaged builds
// keep the normal windowed subsystem and do not open a console.
#![cfg_attr(
    all(not(debug_assertions), not(feature = "task-057-diagnostics")),
    windows_subsystem = "windows"
)]

fn main() {
    m590_ui_lib::run();
}
