use serde::{Deserialize, Serialize};

use super::error::FieldError;
use super::schedule::ScheduleInput;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JobInput {
    pub label: String,
    pub command: String,
    pub schedule: ScheduleInput,
}

pub const MSG_LABEL_EMPTY: &str = "名前を入力してください";
pub const MSG_LABEL_TOO_LONG: &str = "名前は200文字以内で入力してください";
pub const MSG_LABEL_CHARS: &str = "名前に使える文字は英数字と . _ - のみです";
pub const MSG_LABEL_DOT: &str = "名前を . で始めることはできません";
pub const MSG_LABEL_DUP: &str = "同じ名前のジョブがすでにあります";
pub const MSG_COMMAND_EMPTY: &str = "コマンドを入力してください";
pub const MSG_HOUR: &str = "時は0〜23で指定してください";
pub const MSG_MINUTE: &str = "分は0〜59で指定してください";
pub const MSG_WEEKDAYS: &str = "曜日を1つ以上選択してください";
pub const MSG_DAY: &str = "日付は1〜31で指定してください";
pub const MSG_EVERY: &str = "間隔は1分以上で指定してください";
pub const MSG_DAY_WARNING: &str = "29〜31日を指定すると、その日がない月には実行されません";

const LABEL_MAX: usize = 200;

pub fn label_format_error(label: &str) -> Option<String> {
    if label.is_empty() {
        return Some(MSG_LABEL_EMPTY.into());
    }
    if label.chars().count() > LABEL_MAX {
        return Some(MSG_LABEL_TOO_LONG.into());
    }
    if !label.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')) {
        return Some(MSG_LABEL_CHARS.into());
    }
    if label.starts_with('.') {
        return Some(MSG_LABEL_DOT.into());
    }
    None
}

fn check_time(errors: &mut Vec<FieldError>, hour: i64, minute: i64) {
    if !(0..=23).contains(&hour) {
        errors.push(FieldError::new("schedule.hour", MSG_HOUR));
    }
    if !(0..=59).contains(&minute) {
        errors.push(FieldError::new("schedule.minute", MSG_MINUTE));
    }
}

pub fn validate_input(input: &JobInput, existing_labels: &[String], original_label: Option<&str>) -> Vec<FieldError> {
    let mut errors = Vec::new();
    if let Some(m) = label_format_error(&input.label) {
        errors.push(FieldError::new("label", &m));
    } else {
        let lower = input.label.to_lowercase();
        let is_self = original_label.map(|o| o.to_lowercase() == lower).unwrap_or(false);
        if !is_self && existing_labels.iter().any(|l| l.to_lowercase() == lower) {
            errors.push(FieldError::new("label", MSG_LABEL_DUP));
        }
    }
    if input.command.trim().is_empty() {
        errors.push(FieldError::new("command", MSG_COMMAND_EMPTY));
    }
    match &input.schedule {
        ScheduleInput::Daily { hour, minute } => check_time(&mut errors, *hour, *minute),
        ScheduleInput::Weekly { weekdays, hour, minute } => {
            if weekdays.is_empty() || weekdays.iter().any(|d| !(0..=7).contains(d)) {
                errors.push(FieldError::new("schedule.weekdays", MSG_WEEKDAYS));
            }
            check_time(&mut errors, *hour, *minute);
        }
        ScheduleInput::Monthly { day, hour, minute } => {
            if !(1..=31).contains(day) {
                errors.push(FieldError::new("schedule.day", MSG_DAY));
            }
            check_time(&mut errors, *hour, *minute);
        }
        ScheduleInput::Interval { every, .. } => {
            if !(1..=1_000_000).contains(every) {
                errors.push(FieldError::new("schedule.every", MSG_EVERY));
            }
        }
    }
    errors
}

pub fn schedule_warnings(s: &ScheduleInput) -> Vec<String> {
    match s {
        ScheduleInput::Monthly { day, .. } if (29..=31).contains(day) => vec![MSG_DAY_WARNING.into()],
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::plist_model::{job_from_dict, parse_plist_bytes, serialize_xml};
    use crate::domain::schedule::IntervalUnit;
    use plist::{Dictionary, Value};

    fn input(label: &str, command: &str, schedule: ScheduleInput) -> JobInput {
        JobInput { label: label.into(), command: command.into(), schedule }
    }

    fn daily() -> ScheduleInput {
        ScheduleInput::Daily { hour: 9, minute: 0 }
    }

    fn fields(errs: &[FieldError]) -> Vec<&str> {
        errs.iter().map(|e| e.field.as_str()).collect()
    }

    #[test]
    fn label_formats() {
        for bad in ["", "a b", "a/b", "../evil", ".hidden", "..", "日本語"] {
            assert!(label_format_error(bad).is_some(), "{bad}");
        }
        assert_eq!(label_format_error(&"a".repeat(201)).as_deref(), Some(MSG_LABEL_TOO_LONG));
        assert_eq!(label_format_error(&"a".repeat(200)), None);
        assert_eq!(label_format_error("com.example.job_1-2"), None);
        assert_eq!(label_format_error(".x").as_deref(), Some(MSG_LABEL_DOT));
    }

    #[test]
    fn duplicates_case_insensitive_and_self_allowed() {
        let existing = vec!["com.x".to_string()];
        let errs = validate_input(&input("COM.X", "echo", daily()), &existing, None);
        assert_eq!(errs, vec![FieldError::new("label", MSG_LABEL_DUP)]);
        assert!(validate_input(&input("com.x", "echo", daily()), &existing, Some("com.x")).is_empty());
    }

    #[test]
    fn command_empty() {
        let errs = validate_input(&input("a", "  \n", daily()), &[], None);
        assert_eq!(fields(&errs), vec!["command"]);
    }

    #[test]
    fn out_of_range() {
        let v = |s| fields(&validate_input(&input("a", "x", s), &[], None)).iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(v(ScheduleInput::Daily { hour: -1, minute: 0 }), vec!["schedule.hour"]);
        assert_eq!(v(ScheduleInput::Daily { hour: 24, minute: 0 }), vec!["schedule.hour"]);
        assert_eq!(v(ScheduleInput::Daily { hour: 0, minute: 60 }), vec!["schedule.minute"]);
        assert_eq!(v(ScheduleInput::Weekly { weekdays: vec![], hour: 0, minute: 0 }), vec!["schedule.weekdays"]);
        assert_eq!(v(ScheduleInput::Weekly { weekdays: vec![8], hour: 0, minute: 0 }), vec!["schedule.weekdays"]);
        assert_eq!(v(ScheduleInput::Monthly { day: 0, hour: 0, minute: 0 }), vec!["schedule.day"]);
        assert_eq!(v(ScheduleInput::Monthly { day: 32, hour: 0, minute: 0 }), vec!["schedule.day"]);
        assert_eq!(v(ScheduleInput::Interval { every: 0, unit: IntervalUnit::Minutes }), vec!["schedule.every"]);
    }

    #[test]
    fn boundaries_ok() {
        for s in [
            ScheduleInput::Daily { hour: 0, minute: 0 },
            ScheduleInput::Daily { hour: 23, minute: 59 },
            ScheduleInput::Monthly { day: 31, hour: 0, minute: 0 },
            ScheduleInput::Interval { every: 1, unit: IntervalUnit::Minutes },
            ScheduleInput::Weekly { weekdays: vec![0, 7], hour: 0, minute: 0 },
        ] {
            assert!(validate_input(&input("a", "x", s.clone()), &[], None).is_empty(), "{s:?}");
        }
    }

    #[test]
    fn day_warning() {
        assert_eq!(schedule_warnings(&ScheduleInput::Monthly { day: 29, hour: 0, minute: 0 }), vec![MSG_DAY_WARNING]);
        assert!(schedule_warnings(&ScheduleInput::Monthly { day: 28, hour: 0, minute: 0 }).is_empty());
    }

    #[test]
    fn command_escaping_round_trip() {
        let commands = [
            r#"echo "a b" 'c' $HOME && printf '%s\n' "日本語""#,
            "echo 1\necho 2",
            "echo <tag>&amp; > /dev/null",
            "printf 'a\tb'\t# tab",
        ];
        for cmd in commands {
            let mut d = Dictionary::new();
            d.insert(
                "ProgramArguments".into(),
                Value::Array(vec![Value::String("/w".into()), Value::String("l".into()), Value::String(cmd.into())]),
            );
            let bytes = serialize_xml(&d).unwrap();
            let job = job_from_dict(parse_plist_bytes(&bytes).unwrap());
            assert_eq!(job.program_arguments.unwrap()[2], cmd);
        }
    }
}
