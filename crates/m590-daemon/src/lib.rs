//! Library surface for embedding hub/status in the desktop shell.

pub mod config;
pub mod discovery;
pub mod file_save;
pub mod hub;
#[cfg(target_os = "linux")]
pub mod linux_virtual_file;
#[cfg(target_os = "linux")]
pub mod linux_virtual_file_manager;
pub mod status;
pub mod virtual_file_bridge;
#[cfg(target_os = "windows")]
pub mod windows_virtual_file_manager;
