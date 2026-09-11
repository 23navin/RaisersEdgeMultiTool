// multitool-core — the engine shared by the desktop (Tauri) and web (server)
// shells. Everything here is framework-neutral: plain functions over paths and
// serde types. Shell-specific concerns (IPC, HTTP, OAuth loopback, dialogs)
// stay in the shells.

pub mod api;
pub mod creds;
pub mod errors;
pub mod profile;
pub mod validate;
pub mod db;
pub mod re_calls;
pub mod code_tables;
pub mod query_step;
pub mod user_input;
pub mod report;
pub mod workspace;
