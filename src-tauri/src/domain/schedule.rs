use plist::{Dictionary, Value};
use serde::{Deserialize, Serialize};

use super::plist_model::{CalendarEntry, RawSchedule};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IntervalUnit {
    Minutes,
    Hours,
}

/// フォームで扱えるスケジュール。数値は不正値でも受け取れるよう i64。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ScheduleInput {
    Daily { hour: i64, minute: i64 },
    Weekly { weekdays: Vec<i64>, hour: i64, minute: i64 },
    Monthly { day: i64, hour: i64, minute: i64 },
    Interval { every: i64, unit: IntervalUnit },
}

const WEEKDAY_NAMES: [&str; 7] = ["日", "月", "火", "水", "木", "金", "土"];

/// 7→0、重複除去、昇順（0=日）。
pub fn normalize_weekdays(w: &[i64]) -> Vec<i64> {
    let mut v: Vec<i64> = w.iter().map(|&d| if d == 7 { 0 } else { d }).collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// 表示順（月→日）のソートキー。
fn weekday_order(d: i64) -> i64 {
    (d + 6) % 7
}

fn weekday_name(d: i64) -> String {
    match d {
        0..=7 => WEEKDAY_NAMES[(d % 7) as usize].to_string(),
        _ => d.to_string(),
    }
}

fn hm(hour: i64, minute: i64) -> String {
    format!("{hour:02}:{minute:02}")
}

fn in_range(v: Option<i64>, lo: i64, hi: i64) -> bool {
    v.map(|x| (lo..=hi).contains(&x)).unwrap_or(true)
}

fn entry_valid(e: &CalendarEntry) -> bool {
    in_range(e.month, 1, 12)
        && in_range(e.day, 1, 31)
        && in_range(e.weekday, 0, 7)
        && in_range(e.hour, 0, 23)
        && in_range(e.minute, 0, 59)
}

fn fallback(e: &CalendarEntry) -> String {
    let f = |v: Option<i64>| v.map(|x| x.to_string()).unwrap_or_else(|| "*".into());
    let mut parts = vec![format!("月={}", f(e.month)), format!("日={}", f(e.day))];
    if let Some(w) = e.weekday {
        parts.push(format!("曜日={}", weekday_name(w)));
    }
    parts.push(format!("時={}", f(e.hour)));
    parts.push(format!("分={}", f(e.minute)));
    parts.join(", ")
}

enum Item {
    Text(String),
    Weekly { days: Vec<i64>, hour: i64, minute: i64 },
}

fn classify(e: &CalendarEntry) -> Item {
    if !entry_valid(e) {
        return Item::Text(fallback(e));
    }
    match (e.month, e.day, e.weekday, e.hour, e.minute) {
        (None, None, None, Some(h), Some(m)) => Item::Text(format!("毎日 {}", hm(h, m))),
        (None, None, Some(w), Some(h), Some(m)) => Item::Weekly { days: vec![w % 7], hour: h, minute: m },
        (None, Some(d), None, Some(h), Some(m)) => Item::Text(format!("毎月 {}日 {}", d, hm(h, m))),
        (None, None, None, None, Some(m)) => Item::Text(format!("毎時 {m:02}分")),
        _ => Item::Text(fallback(e)),
    }
}

fn describe_interval(secs: i64) -> String {
    if secs > 0 && secs % 3600 == 0 {
        format!("{}時間ごと", secs / 3600)
    } else if secs > 0 && secs % 60 == 0 {
        format!("{}分ごと", secs / 60)
    } else {
        format!("{secs}秒ごと")
    }
}

/// スケジュールを統一フォーマットの日本語に変換する。
pub fn describe(raw: &RawSchedule) -> Vec<String> {
    let mut items: Vec<Item> = Vec::new();
    for e in &raw.calendar {
        match classify(e) {
            Item::Weekly { days, hour, minute } => {
                // 同じ時刻の毎週指定はひとつにまとめる
                let existing = items.iter_mut().find_map(|it| match it {
                    Item::Weekly { days: d, hour: h, minute: m } if *h == hour && *m == minute => Some(d),
                    _ => None,
                });
                match existing {
                    Some(d) => d.extend(days),
                    None => items.push(Item::Weekly { days, hour, minute }),
                }
            }
            t => items.push(t),
        }
    }
    let mut lines: Vec<String> = items
        .into_iter()
        .map(|it| match it {
            Item::Text(s) => s,
            Item::Weekly { mut days, hour, minute } => {
                days.sort_by_key(|&d| weekday_order(d));
                days.dedup();
                let names: Vec<String> = days.iter().map(|&d| weekday_name(d)).collect();
                format!("毎週 {} {}", names.join("・"), hm(hour, minute))
            }
        })
        .collect();
    if let Some(s) = raw.interval_secs {
        lines.push(describe_interval(s));
    }
    if raw.run_at_load {
        lines.push("ログイン時に実行".into());
    }
    if lines.is_empty() {
        lines.push("スケジュール指定なし".into());
    }
    lines
}

fn cal_dict(pairs: &[(&str, i64)]) -> Value {
    let mut d = Dictionary::new();
    for (k, v) in pairs {
        d.insert((*k).into(), Value::Integer((*v).into()));
    }
    Value::Dictionary(d)
}

/// 既存の StartCalendarInterval / StartInterval を置き換える。RunAtLoad 等は保持する。
/// 同じキーを置き換える場合は元の位置を保つ。
pub fn apply_schedule(dict: &mut Dictionary, s: &ScheduleInput) {
    let (key, value) = match s {
        ScheduleInput::Daily { hour, minute } => {
            ("StartCalendarInterval", cal_dict(&[("Hour", *hour), ("Minute", *minute)]))
        }
        ScheduleInput::Weekly { weekdays, hour, minute } => {
            let days = normalize_weekdays(weekdays);
            let mut entries: Vec<Value> = days
                .iter()
                .map(|&w| cal_dict(&[("Weekday", w), ("Hour", *hour), ("Minute", *minute)]))
                .collect();
            let v = if entries.len() == 1 { entries.remove(0) } else { Value::Array(entries) };
            ("StartCalendarInterval", v)
        }
        ScheduleInput::Monthly { day, hour, minute } => (
            "StartCalendarInterval",
            cal_dict(&[("Day", *day), ("Hour", *hour), ("Minute", *minute)]),
        ),
        ScheduleInput::Interval { every, unit } => {
            let secs = match unit {
                IntervalUnit::Minutes => every * 60,
                IntervalUnit::Hours => every * 3600,
            };
            ("StartInterval", Value::Integer(secs.into()))
        }
    };
    let other = if key == "StartInterval" { "StartCalendarInterval" } else { "StartInterval" };
    // Dictionary::remove は swap_remove で順序が崩れるため retain を使う
    dict.retain(|k, _| k != other);
    dict.insert(key.into(), value);
}

/// plist のスケジュールをフォーム値に戻す。表現できない形なら None。
pub fn to_form(raw: &RawSchedule) -> Option<ScheduleInput> {
    if !raw.calendar.iter().all(entry_valid) {
        return None;
    }
    match (raw.calendar.as_slice(), raw.interval_secs) {
        ([], Some(s)) if s > 0 && s % 60 == 0 => Some(if s % 3600 == 0 {
            ScheduleInput::Interval { every: s / 3600, unit: IntervalUnit::Hours }
        } else {
            ScheduleInput::Interval { every: s / 60, unit: IntervalUnit::Minutes }
        }),
        ([e], None) => match (e.month, e.day, e.weekday, e.hour, e.minute) {
            (None, None, None, Some(hour), Some(minute)) => Some(ScheduleInput::Daily { hour, minute }),
            (None, Some(day), None, Some(hour), Some(minute)) => Some(ScheduleInput::Monthly { day, hour, minute }),
            (None, None, Some(w), Some(hour), Some(minute)) => Some(ScheduleInput::Weekly {
                weekdays: normalize_weekdays(&[w]),
                hour,
                minute,
            }),
            _ => None,
        },
        (entries, None) if entries.len() > 1 => {
            let (h, m) = (entries[0].hour?, entries[0].minute?);
            let mut days = Vec::new();
            for e in entries {
                match (e.month, e.day, e.weekday, e.hour, e.minute) {
                    (None, None, Some(w), Some(eh), Some(em)) if eh == h && em == m => days.push(w),
                    _ => return None,
                }
            }
            Some(ScheduleInput::Weekly { weekdays: normalize_weekdays(&days), hour: h, minute: m })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::plist_model::{job_from_dict, parse_plist_bytes, serialize_xml};

    fn e(month: Option<i64>, day: Option<i64>, weekday: Option<i64>, hour: Option<i64>, minute: Option<i64>) -> CalendarEntry {
        CalendarEntry { month, day, weekday, hour, minute }
    }

    fn cal(entries: Vec<CalendarEntry>) -> RawSchedule {
        RawSchedule { calendar: entries, ..Default::default() }
    }

    #[test]
    fn daily() {
        assert_eq!(describe(&cal(vec![e(None, None, None, Some(9), Some(0))])), vec!["毎日 09:00"]);
    }

    #[test]
    fn weekly_single_and_multiple() {
        assert_eq!(describe(&cal(vec![e(None, None, Some(1), Some(9), Some(0))])), vec!["毎週 月 09:00"]);
        let r = cal(vec![
            e(None, None, Some(5), Some(9), Some(0)),
            e(None, None, Some(1), Some(9), Some(0)),
            e(None, None, Some(3), Some(9), Some(0)),
        ]);
        assert_eq!(describe(&r), vec!["毎週 月・水・金 09:00"]);
    }

    #[test]
    fn weekday_zero_and_seven_are_sunday() {
        assert_eq!(describe(&cal(vec![e(None, None, Some(0), Some(8), Some(5))])), vec!["毎週 日 08:05"]);
        assert_eq!(describe(&cal(vec![e(None, None, Some(7), Some(8), Some(5))])), vec!["毎週 日 08:05"]);
        let r = cal(vec![e(None, None, Some(0), Some(8), Some(5)), e(None, None, Some(7), Some(8), Some(5))]);
        assert_eq!(describe(&r), vec!["毎週 日 08:05"]);
    }

    #[test]
    fn monthly_hourly_interval_runatload() {
        assert_eq!(describe(&cal(vec![e(None, Some(1), None, Some(9), Some(0))])), vec!["毎月 1日 09:00"]);
        assert_eq!(describe(&cal(vec![e(None, None, None, None, Some(15))])), vec!["毎時 15分"]);
        assert_eq!(describe(&RawSchedule { interval_secs: Some(1800), ..Default::default() }), vec!["30分ごと"]);
        assert_eq!(describe(&RawSchedule { interval_secs: Some(7200), ..Default::default() }), vec!["2時間ごと"]);
        assert_eq!(describe(&RawSchedule { interval_secs: Some(45), ..Default::default() }), vec!["45秒ごと"]);
        assert_eq!(describe(&RawSchedule { run_at_load: true, ..Default::default() }), vec!["ログイン時に実行"]);
        assert_eq!(describe(&RawSchedule::default()), vec!["スケジュール指定なし"]);
    }

    #[test]
    fn array_lists_all_entries() {
        let r = RawSchedule {
            calendar: vec![
                e(None, None, None, Some(9), Some(0)),
                e(None, Some(15), None, Some(12), Some(30)),
                e(None, None, Some(6), Some(10), Some(0)),
            ],
            interval_secs: None,
            run_at_load: true,
        };
        assert_eq!(describe(&r), vec!["毎日 09:00", "毎月 15日 12:30", "毎週 土 10:00", "ログイン時に実行"]);
    }

    #[test]
    fn fallbacks() {
        assert_eq!(describe(&cal(vec![e(None, Some(1), None, None, Some(0))])), vec!["月=*, 日=1, 時=*, 分=0"]);
        assert_eq!(describe(&cal(vec![e(Some(3), Some(1), None, Some(9), Some(0))])), vec!["月=3, 日=1, 時=9, 分=0"]);
        assert_eq!(describe(&cal(vec![e(None, None, None, None, None)])), vec!["月=*, 日=*, 時=*, 分=*"]);
        assert_eq!(describe(&cal(vec![e(None, None, Some(9), Some(9), Some(0))])), vec!["月=*, 日=*, 曜日=9, 時=9, 分=0"]);
        assert_eq!(describe(&cal(vec![e(None, Some(1), Some(1), Some(9), Some(0))])), vec!["月=*, 日=1, 曜日=月, 時=9, 分=0"]);
        assert_eq!(describe(&cal(vec![e(None, None, None, Some(25), Some(0))])), vec!["月=*, 日=*, 時=25, 分=0"]);
    }

    #[test]
    fn boundaries() {
        assert_eq!(describe(&cal(vec![e(None, None, None, Some(0), Some(0))])), vec!["毎日 00:00"]);
        assert_eq!(describe(&cal(vec![e(None, None, None, Some(23), Some(59))])), vec!["毎日 23:59"]);
        assert_eq!(describe(&cal(vec![e(None, Some(31), None, Some(0), Some(0))])), vec!["毎月 31日 00:00"]);
        assert_eq!(describe(&RawSchedule { interval_secs: Some(60), ..Default::default() }), vec!["1分ごと"]);
    }

    fn round_trip(s: &ScheduleInput) -> Option<ScheduleInput> {
        let mut d = Dictionary::new();
        d.insert("Label".into(), Value::String("x".into()));
        apply_schedule(&mut d, s);
        let bytes = serialize_xml(&d).unwrap();
        let job = job_from_dict(parse_plist_bytes(&bytes).unwrap());
        to_form(&job.schedule)
    }

    #[test]
    fn round_trips() {
        let cases = vec![
            ScheduleInput::Daily { hour: 0, minute: 0 },
            ScheduleInput::Daily { hour: 23, minute: 59 },
            ScheduleInput::Weekly { weekdays: vec![1, 3, 5], hour: 9, minute: 0 },
            ScheduleInput::Weekly { weekdays: vec![0], hour: 9, minute: 0 },
            ScheduleInput::Monthly { day: 31, hour: 7, minute: 30 },
            ScheduleInput::Interval { every: 1, unit: IntervalUnit::Minutes },
            ScheduleInput::Interval { every: 5, unit: IntervalUnit::Hours },
        ];
        for c in cases {
            assert_eq!(round_trip(&c).as_ref(), Some(&c), "{c:?}");
        }
    }

    #[test]
    fn sixty_minutes_normalizes_to_one_hour() {
        let r = round_trip(&ScheduleInput::Interval { every: 60, unit: IntervalUnit::Minutes });
        assert_eq!(r, Some(ScheduleInput::Interval { every: 1, unit: IntervalUnit::Hours }));
    }

    #[test]
    fn weekly_seven_normalizes() {
        let r = round_trip(&ScheduleInput::Weekly { weekdays: vec![7, 1, 1], hour: 9, minute: 0 });
        assert_eq!(r, Some(ScheduleInput::Weekly { weekdays: vec![0, 1], hour: 9, minute: 0 }));
    }

    #[test]
    fn apply_keeps_run_at_load_and_replaces_old_schedule() {
        let mut d = Dictionary::new();
        d.insert("RunAtLoad".into(), Value::Boolean(true));
        d.insert("StartInterval".into(), Value::Integer(10.into()));
        apply_schedule(&mut d, &ScheduleInput::Daily { hour: 1, minute: 2 });
        assert_eq!(d.get("RunAtLoad"), Some(&Value::Boolean(true)));
        assert!(d.get("StartInterval").is_none());
        assert!(d.get("StartCalendarInterval").is_some());
    }

    #[test]
    fn to_form_unrepresentable_is_none() {
        assert_eq!(to_form(&cal(vec![e(None, None, None, None, Some(15))])), None);
        assert_eq!(to_form(&RawSchedule::default()), None);
        assert_eq!(to_form(&RawSchedule { interval_secs: Some(45), ..Default::default() }), None);
        let mixed = cal(vec![e(None, None, Some(1), Some(9), Some(0)), e(None, None, Some(2), Some(10), Some(0))]);
        assert_eq!(to_form(&mixed), None);
    }

    #[test]
    fn schedule_input_json() {
        let s: ScheduleInput =
            serde_json::from_str(r#"{"kind":"weekly","weekdays":[1,3],"hour":9,"minute":0}"#).unwrap();
        assert_eq!(s, ScheduleInput::Weekly { weekdays: vec![1, 3], hour: 9, minute: 0 });
        let s: ScheduleInput = serde_json::from_str(r#"{"kind":"interval","every":5,"unit":"hours"}"#).unwrap();
        assert_eq!(s, ScheduleInput::Interval { every: 5, unit: IntervalUnit::Hours });
    }
}
