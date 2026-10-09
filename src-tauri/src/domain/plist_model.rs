use std::path::Path;

use plist::{Dictionary, Value};

#[derive(Clone, Debug, PartialEq, Default)]
pub struct CalendarEntry {
    pub month: Option<i64>,
    pub day: Option<i64>,
    pub weekday: Option<i64>,
    pub hour: Option<i64>,
    pub minute: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct RawSchedule {
    pub calendar: Vec<CalendarEntry>,
    pub interval_secs: Option<i64>,
    pub run_at_load: bool,
}

#[derive(Clone, Debug)]
pub struct JobPlist {
    pub label: Option<String>,
    pub program: Option<String>,
    pub program_arguments: Option<Vec<String>>,
    pub schedule: RawSchedule,
    pub stdout_path: Option<String>,
    pub stderr_path: Option<String>,
    pub dict: Dictionary,
}

/// XML / バイナリ plist を読み込む。ルートが辞書でなければエラー。
pub fn parse_plist_bytes(bytes: &[u8]) -> Result<Dictionary, String> {
    let value = Value::from_reader(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
    match value {
        Value::Dictionary(d) => Ok(d),
        _ => Err("plist のルートが辞書ではありません".into()),
    }
}

fn int_of(v: &Value) -> Option<i64> {
    match v {
        Value::Integer(i) => i.as_signed().or_else(|| i.as_unsigned().map(|u| u as i64)),
        Value::Real(r) if r.fract() == 0.0 => Some(*r as i64),
        _ => None,
    }
}

fn calendar_entry(d: &Dictionary) -> CalendarEntry {
    let get = |k: &str| d.get(k).and_then(int_of);
    CalendarEntry {
        month: get("Month"),
        day: get("Day"),
        weekday: get("Weekday"),
        hour: get("Hour"),
        minute: get("Minute"),
    }
}

pub fn raw_schedule(dict: &Dictionary) -> RawSchedule {
    let calendar = match dict.get("StartCalendarInterval") {
        Some(Value::Dictionary(d)) => vec![calendar_entry(d)],
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|v| v.as_dictionary().map(calendar_entry))
            .collect(),
        _ => vec![],
    };
    RawSchedule {
        calendar,
        interval_secs: dict.get("StartInterval").and_then(int_of),
        run_at_load: dict.get("RunAtLoad").and_then(Value::as_boolean).unwrap_or(false),
    }
}

pub fn job_from_dict(dict: Dictionary) -> JobPlist {
    let string = |k: &str| dict.get(k).and_then(Value::as_string).map(str::to_string);
    let program_arguments = dict.get("ProgramArguments").and_then(Value::as_array).map(|a| {
        a.iter()
            .filter_map(|v| v.as_string().map(str::to_string))
            .collect::<Vec<_>>()
    });
    JobPlist {
        label: string("Label"),
        program: string("Program"),
        program_arguments,
        schedule: raw_schedule(&dict),
        stdout_path: string("StandardOutPath"),
        stderr_path: string("StandardErrorPath"),
        dict,
    }
}

pub fn serialize_xml(dict: &Dictionary) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    Value::Dictionary(dict.clone())
        .to_writer_xml(&mut buf)
        .map_err(|e| e.to_string())?;
    if !buf.ends_with(b"\n") {
        buf.push(b'\n');
    }
    Ok(buf)
}

/// `ProgramArguments[0]` がラッパーの絶対パスと一致すれば本アプリ管理とみなす。
pub fn is_managed(job: &JobPlist, wrapper_path: &Path) -> bool {
    match &job.program_arguments {
        Some(args) => args.first().map(|a| Path::new(a) == wrapper_path).unwrap_or(false),
        None => false,
    }
}

pub fn managed_command(job: &JobPlist) -> String {
    job.program_arguments
        .as_ref()
        .and_then(|a| a.get(2).cloned())
        .unwrap_or_default()
}

fn shell_quote(s: &str) -> String {
    let needs = s.is_empty()
        || s.chars().any(|c| c.is_whitespace() || matches!(c, '\'' | '"' | '$' | '\\' | '`'));
    if needs {
        format!("'{}'", s.replace('\'', "'\\''"))
    } else {
        s.to_string()
    }
}

pub fn display_command(job: &JobPlist) -> Option<String> {
    let parts: Vec<String> = match (&job.program, &job.program_arguments) {
        (Some(p), Some(args)) => std::iter::once(p.clone())
            .chain(args.iter().skip(1).cloned())
            .collect(),
        (None, Some(args)) if !args.is_empty() => args.clone(),
        (Some(p), None) => vec![p.clone()],
        _ => return None,
    };
    Some(parts.iter().map(|s| shell_quote(s)).collect::<Vec<_>>().join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn xml(body: &str) -> Vec<u8> {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>{body}</dict></plist>"#
        )
        .into_bytes()
    }

    fn job(body: &str) -> JobPlist {
        job_from_dict(parse_plist_bytes(&xml(body)).unwrap())
    }

    #[test]
    fn reads_xml() {
        let j = job("<key>Label</key><string>com.a</string>");
        assert_eq!(j.label.as_deref(), Some("com.a"));
    }

    #[test]
    fn reads_binary() {
        let mut d = Dictionary::new();
        d.insert("Label".into(), Value::String("com.bin".into()));
        d.insert("StartInterval".into(), Value::Integer(300.into()));
        let mut buf = Vec::new();
        plist::to_writer_binary(&mut buf, &Value::Dictionary(d)).unwrap();
        let j = job_from_dict(parse_plist_bytes(&buf).unwrap());
        assert_eq!(j.label.as_deref(), Some("com.bin"));
        assert_eq!(j.schedule.interval_secs, Some(300));
    }

    #[test]
    fn program_only() {
        let j = job("<key>Program</key><string>/usr/bin/true</string>");
        assert_eq!(display_command(&j).as_deref(), Some("/usr/bin/true"));
    }

    #[test]
    fn program_arguments_only() {
        let j = job("<key>ProgramArguments</key><array><string>/bin/echo</string><string>a b</string><string>it's</string></array>");
        assert_eq!(display_command(&j).as_deref(), Some(r#"/bin/echo 'a b' 'it'\''s'"#));
    }

    #[test]
    fn program_and_arguments() {
        let j = job("<key>Program</key><string>/bin/real</string><key>ProgramArguments</key><array><string>argv0</string><string>x</string></array>");
        assert_eq!(display_command(&j).as_deref(), Some("/bin/real x"));
    }

    #[test]
    fn no_program() {
        assert_eq!(display_command(&job("<key>Label</key><string>a</string>")), None);
    }

    #[test]
    fn calendar_dict() {
        let j = job("<key>StartCalendarInterval</key><dict><key>Hour</key><integer>9</integer><key>Minute</key><integer>0</integer></dict>");
        assert_eq!(
            j.schedule.calendar,
            vec![CalendarEntry { hour: Some(9), minute: Some(0), ..Default::default() }]
        );
    }

    #[test]
    fn calendar_array() {
        let j = job("<key>StartCalendarInterval</key><array><dict><key>Weekday</key><integer>1</integer></dict><dict><key>Day</key><integer>3</integer></dict></array>");
        assert_eq!(j.schedule.calendar.len(), 2);
        assert_eq!(j.schedule.calendar[0].weekday, Some(1));
        assert_eq!(j.schedule.calendar[1].day, Some(3));
    }

    #[test]
    fn start_interval_and_run_at_load() {
        let j = job("<key>StartInterval</key><integer>1800</integer>");
        assert_eq!(j.schedule.interval_secs, Some(1800));
        let j = job("<key>RunAtLoad</key><true/>");
        assert_eq!(j.schedule, RawSchedule { run_at_load: true, ..Default::default() });
        let j = job("<key>Label</key><string>x</string>");
        assert_eq!(j.schedule, RawSchedule::default());
    }

    #[test]
    fn missing_label() {
        let j = job("<key>Program</key><string>/x</string>");
        assert_eq!(j.label, None);
    }

    #[test]
    fn broken_inputs_are_errors_not_panics() {
        assert!(parse_plist_bytes(b"<?xml version=\"1.0\"?><plist><dict><key>").is_err());
        assert!(parse_plist_bytes(&[0u8, 0, 0, 1, 66, 117, 100, 49, 0, 0, 0, 0]).is_err());
        assert!(parse_plist_bytes(b"").is_err());
        let arr = br#"<?xml version="1.0"?><plist version="1.0"><array/></plist>"#;
        assert!(parse_plist_bytes(arr).is_err());
    }

    #[test]
    fn managed_detection() {
        let w = Path::new("/w/run-job.sh");
        let j = job("<key>ProgramArguments</key><array><string>/w/run-job.sh</string><string>l</string><string>echo hi</string></array>");
        assert!(is_managed(&j, w));
        assert_eq!(managed_command(&j), "echo hi");
        let j = job("<key>ProgramArguments</key><array><string>/other/run-job.sh</string></array>");
        assert!(!is_managed(&j, w));
        let j = job("<key>Program</key><string>/w/run-job.sh</string>");
        assert!(!is_managed(&j, w));
        let j = job("<key>ProgramArguments</key><array/>");
        assert!(!is_managed(&j, w));
    }
}
