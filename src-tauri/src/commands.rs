//! Tauri コマンド。JobService を呼ぶだけの薄い層。

use tauri::State;

use crate::domain::dto::{JobDetail, JobSummary};
use crate::domain::error::AppError;
use crate::domain::service::JobService;
use crate::domain::validate::JobInput;

pub struct AppState {
    pub service: JobService,
}

#[tauri::command]
pub async fn list_jobs(state: State<'_, AppState>) -> Result<Vec<JobSummary>, AppError> {
    state.service.list_jobs()
}

#[tauri::command]
pub async fn get_job_detail(state: State<'_, AppState>, file_name: String) -> Result<JobDetail, AppError> {
    state.service.get_job_detail(&file_name)
}

#[tauri::command]
pub async fn create_job(state: State<'_, AppState>, input: JobInput) -> Result<String, AppError> {
    state.service.create_job(input)
}

#[tauri::command]
pub async fn update_job(state: State<'_, AppState>, file_name: String, input: JobInput) -> Result<String, AppError> {
    state.service.update_job(&file_name, input)
}

#[tauri::command]
pub async fn delete_job(state: State<'_, AppState>, file_name: String) -> Result<(), AppError> {
    state.service.delete_job(&file_name)
}

#[tauri::command]
pub async fn set_job_enabled(state: State<'_, AppState>, file_name: String, enabled: bool) -> Result<(), AppError> {
    state.service.set_enabled(&file_name, enabled)
}

#[tauri::command]
pub async fn run_job_now(state: State<'_, AppState>, file_name: String) -> Result<(), AppError> {
    state.service.run_now(&file_name)
}
