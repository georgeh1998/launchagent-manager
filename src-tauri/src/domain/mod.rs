//! tauri に依存しない純粋ロジックと副作用の抽象化。

pub mod clock;
pub mod cmd;
pub mod config;
pub mod dto;
pub mod error;
pub mod fakes;
pub mod fs;
pub mod launchctl;
pub mod plist_model;
pub mod run_record;
pub mod schedule;
pub mod service;
pub mod validate;
pub mod wrapper;
