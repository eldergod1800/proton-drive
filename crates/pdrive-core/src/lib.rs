pub mod auth;
pub mod config;
pub mod db;
pub mod drive;
pub mod sync;

// Re-export for crates that use DriveClient without depending on the SDK directly
pub use proton_drive_sdk::node::NodeUid;
