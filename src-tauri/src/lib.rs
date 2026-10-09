pub mod commands;
pub mod domain;

use std::sync::Arc;

use commands::AppState;
use domain::clock::SystemClock;
use domain::cmd::RealRunner;
use domain::config::Config;
use domain::fs::RealFs;
use domain::service::JobService;

pub fn run() {
    let cfg = Config::from_env().expect("failed to resolve HOME");
    let service = JobService::new(Arc::new(RealFs), Arc::new(RealRunner), Arc::new(SystemClock), cfg);
    if let Err(e) = service.ensure_wrapper() {
        eprintln!("failed to install wrapper script: {e}");
    }

    tauri::Builder::default()
        .manage(AppState { service })
        .invoke_handler(tauri::generate_handler![
            commands::list_jobs,
            commands::get_job_detail,
            commands::create_job,
            commands::update_job,
            commands::delete_job,
            commands::set_job_enabled,
            commands::run_job_now,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
