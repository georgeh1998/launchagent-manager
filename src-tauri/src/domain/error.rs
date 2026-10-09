use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FieldError {
    pub field: String,
    pub message: String,
}

impl FieldError {
    pub fn new(field: &str, message: &str) -> Self {
        FieldError { field: field.into(), message: message.into() }
    }
}

#[derive(Clone, Debug, PartialEq, thiserror::Error, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum AppError {
    #[error("入力内容に誤りがあります")]
    Validation { errors: Vec<FieldError> },
    #[error("本アプリで作成したジョブではありません: {file_name}")]
    NotManaged { file_name: String },
    #[error("ジョブが見つかりません: {file_name}")]
    NotFound { file_name: String },
    #[error("不正なパスです: {path}")]
    InvalidPath { path: String },
    #[error("launchctl {operation} が失敗しました（終了コード {exit_code}）: {stderr}")]
    Launchctl { operation: String, exit_code: i32, stderr: String, rolled_back: bool },
    #[error("{message}")]
    Io { message: String },
    #[error("{message}")]
    Plist { message: String },
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Io { message: e.to_string() }
    }
}

impl AppError {
    pub fn with_rolled_back(self, rolled: bool) -> Self {
        match self {
            AppError::Launchctl { operation, exit_code, stderr, .. } => {
                AppError::Launchctl { operation, exit_code, stderr, rolled_back: rolled }
            }
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launchctl_error_json_shape() {
        let e = AppError::Launchctl {
            operation: "bootstrap".into(),
            exit_code: 5,
            stderr: "x".into(),
            rolled_back: true,
        };
        assert_eq!(
            serde_json::to_value(&e).unwrap(),
            serde_json::json!({"kind":"launchctl","operation":"bootstrap","exitCode":5,"stderr":"x","rolledBack":true})
        );
    }

    #[test]
    fn other_error_json_shapes() {
        let e = AppError::NotManaged { file_name: "a.plist".into() };
        assert_eq!(
            serde_json::to_value(&e).unwrap(),
            serde_json::json!({"kind":"notManaged","fileName":"a.plist"})
        );
        let e = AppError::Validation { errors: vec![FieldError::new("label", "m")] };
        assert_eq!(
            serde_json::to_value(&e).unwrap(),
            serde_json::json!({"kind":"validation","errors":[{"field":"label","message":"m"}]})
        );
    }
}
