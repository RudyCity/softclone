//! Cross-platform file operations and persistent clone registry.
pub mod core;
pub mod instance_manager;
pub mod registry;
pub mod rune_engine;
pub mod universal;
#[cfg(target_os = "windows")]
pub mod windows_installed;
#[cfg(target_os = "windows")]
pub mod windows_store;
