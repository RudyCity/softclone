//! Cross-platform file operations, persistent clone registry and local diagnostics.
//! This library intentionally has no GUI toolkit dependency.
pub mod core;
pub mod diagnostics;
pub mod instance_manager;
pub mod registry;
pub mod rune_engine;
pub mod universal;
#[cfg(target_os = "windows")]
pub mod windows_installed;
#[cfg(target_os = "windows")]
pub mod windows_store;
