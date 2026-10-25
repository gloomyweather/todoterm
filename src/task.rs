//! Task model and the inline `!priority @due` syntax.

use chrono::{DateTime, Datelike, Days, Local, NaiveDate, Weekday};
use serde::{Deserialize, Serialize};

pub const MAX_PRIORITY: u8 = 3;

/// A single task. Fields added after 0.1 are optional so old files load as-is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Task {
    pub id: u32,
    pub description: String,
    pub done: bool,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub priority: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due: Option<NaiveDate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<DateTime<Local>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed: Option<DateTime<Local>>,
}

fn is_zero(v: &u8) -> bool {
    *v == 0
}

impl Task {
    pub fn new(id: u32, parsed: Parsed) -> Self {
        Self {
            id,
            description: parsed.text,
            done: false,
            priority: parsed.priority,
            due: parsed.due,
            created: Some(Local::now()),
            completed: None,
        }
    }

    pub fn set_done(&mut self, done: bool) {
        self.done = done;
        self.completed = done.then(Local::now);
    }

    /// Text for the line editor: description followed by the task's tokens.
    pub fn to_input(&self) -> String {
        let mut s = self.description.clone();
        if self.priority > 0 {
            s.push(' ');
            s.push_str(&"!".repeat(self.priority as usize));
        }
        if let Some(due) = self.due {
            s.push_str(&format!(" @{}", due.format("%Y-%m-%d")));
        }
        s
    }

    pub fn matches(&self, needle_lower: &str) -> bool {
        needle_lower.is_empty() || self.description.to_lowercase().contains(needle_lower)
    }
}

/// Result of parsing a line of user input.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Parsed {
    pub text: String,
    pub priority: u8,
    pub due: Option<NaiveDate>,
}

/// Parse `buy milk !! @fri` into text, priority and due date.
/// Unrecognised `@tokens` stay part of the text.
pub fn parse_input(input: &str, today: NaiveDate) -> Parsed {
    let mut parsed = Parsed::default();
    let mut words = Vec::new();
    for word in input.split_whitespace() {
        if !word.is_empty() && word.len() <= MAX_PRIORITY as usize && word.chars().all(|c| c == '!') {
            parsed.priority = word.len() as u8;
        } else if let Some(due) = word.strip_prefix('@').and_then(|d| parse_due(d, today)) {
            parsed.due = Some(due);
        } else {
            words.push(word);
        }
    }
    parsed.text = words.join(" ");
    parsed
}

/// Parse the part after `@`: today, tom, mon…sun, +3d, +2w, 2026-11-01, 11-01.
pub fn parse_due(s: &str, today: NaiveDate) -> Option<NaiveDate> {
    let s = s.to_lowercase();
    match s.as_str() {
        "today" | "tod" => return Some(today),
        "tom" | "tmr" | "tomorrow" => return today.checked_add_days(Days::new(1)),
        _ => {}
    }
    if let Some(wd) = parse_weekday(&s) {
        // The next such day, never today: "@fri" on a friday means next week.
        let ahead = (wd.num_days_from_monday() + 7 - today.weekday().num_days_from_monday()) % 7;
        let ahead = if ahead == 0 { 7 } else { ahead };
        return today.checked_add_days(Days::new(ahead as u64));
    }
    if let Some(rel) = s.strip_prefix('+') {
        let (num, mult) = match rel.chars().last()? {
            'd' => (&rel[..rel.len() - 1], 1),
            'w' => (&rel[..rel.len() - 1], 7),
            c if c.is_ascii_digit() => (rel, 1),
            _ => return None,
        };
        let n: u64 = num.parse().ok()?;
        return today.checked_add_days(Days::new(n.checked_mul(mult)?));
    }
    if let Ok(date) = NaiveDate::parse_from_str(&s, "%Y-%m-%d") {
        return Some(date);
    }
    // MM-DD: this year, or next year if that date has already passed.
    let (m, d) = s.split_once('-')?;
    if m.len() > 2 || d.len() > 2 {
        return None;
    }
    let (m, d): (u32, u32) = (m.parse().ok()?, d.parse().ok()?);
    let this_year = NaiveDate::from_ymd_opt(today.year(), m, d)?;
    if this_year >= today {
        Some(this_year)
    } else {
        NaiveDate::from_ymd_opt(today.year() + 1, m, d)
    }
}

fn parse_weekday(s: &str) -> Option<Weekday> {
    Some(match s {
        "mon" | "monday" => Weekday::Mon,
        "tue" | "tues" | "tuesday" => Weekday::Tue,
        "wed" | "wednesday" => Weekday::Wed,
        "thu" | "thur" | "thurs" | "thursday" => Weekday::Thu,
        "fri" | "friday" => Weekday::Fri,
        "sat" | "saturday" => Weekday::Sat,
        "sun" | "sunday" => Weekday::Sun,
        _ => return None,
    })
}

/// How urgent a due date is relative to today, for colouring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Urgency {
    Overdue,
    Today,
    Later,
}

pub fn urgency(due: NaiveDate, today: NaiveDate) -> Urgency {
    match due.cmp(&today) {
        std::cmp::Ordering::Less => Urgency::Overdue,
        std::cmp::Ordering::Equal => Urgency::Today,
        std::cmp::Ordering::Greater => Urgency::Later,
    }
}

/// Short human label for a due date: today, tomorrow, fri, nov 01, 2027-01-05.
pub fn format_due(due: NaiveDate, today: NaiveDate) -> String {
    let days = (due - today).num_days();
    match days {
        0 => "today".into(),
        1 => "tomorrow".into(),
        -1 => "yesterday".into(),
        2..=6 => due.format("%a").to_string().to_lowercase(),
        _ if due.year() == today.year() => due.format("%b %d").to_string().to_lowercase(),
        _ => due.format("%Y-%m-%d").to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Thursday.
    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 22).unwrap()
    }

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn plain_text() {
        let p = parse_input("  buy   milk ", today());
        assert_eq!(p.text, "buy milk");
        assert_eq!(p.priority, 0);
        assert_eq!(p.due, None);
    }

    #[test]
    fn priority_and_due() {
        let p = parse_input("buy milk !! @fri", today());
        assert_eq!(p.text, "buy milk");
        assert_eq!(p.priority, 2);
        assert_eq!(p.due, Some(d(2026, 10, 23)));
        assert_eq!(parse_input("! a", today()).priority, 1);
        assert_eq!(parse_input("a !!!", today()).priority, 3);
    }

    #[test]
    fn too_many_bangs_is_text() {
        let p = parse_input("wow !!!!", today());
        assert_eq!(p.text, "wow !!!!");
        assert_eq!(p.priority, 0);
    }

    #[test]
    fn unknown_at_token_stays() {
        let p = parse_input("email @bob about @lunch", today());
        assert_eq!(p.text, "email @bob about @lunch");
        assert_eq!(p.due, None);
    }

    #[test]
    fn due_keywords() {
        let t = today();
        assert_eq!(parse_due("today", t), Some(t));
        assert_eq!(parse_due("TOM", t), Some(d(2026, 10, 23)));
        assert_eq!(parse_due("tomorrow", t), Some(d(2026, 10, 23)));
        assert_eq!(parse_due("mon", t), Some(d(2026, 10, 26)));
        // Same weekday as today means next week.
        assert_eq!(parse_due("thu", t), Some(d(2026, 10, 29)));
        assert_eq!(parse_due("+3d", t), Some(d(2026, 10, 25)));
        assert_eq!(parse_due("+2w", t), Some(d(2026, 11, 5)));
        assert_eq!(parse_due("+1", t), Some(d(2026, 10, 23)));
        assert_eq!(parse_due("2026-11-01", t), Some(d(2026, 11, 1)));
        assert_eq!(parse_due("11-01", t), Some(d(2026, 11, 1)));
        // Already passed this year: next year.
        assert_eq!(parse_due("01-15", t), Some(d(2027, 1, 15)));
        assert_eq!(parse_due("10-22", t), Some(t));
    }

    #[test]
    fn due_garbage() {
        let t = today();
        for s in [
            "",
            "+",
            "+d",
            "+3x",
            "13-01",
            "02-30",
            "2026-13-01",
            "bob",
            "123-1",
        ] {
            assert_eq!(parse_due(s, t), None, "{s}");
        }
    }

    #[test]
    fn roundtrip_input() {
        let t = today();
        let task = Task::new(1, parse_input("call mom !!! @tom", t));
        let again = parse_input(&task.to_input(), t);
        assert_eq!(again.text, "call mom");
        assert_eq!(again.priority, 3);
        assert_eq!(again.due, Some(d(2026, 10, 23)));
    }

    #[test]
    fn old_format_loads() {
        let json = r#"[{"id":1,"description":"old","done":true}]"#;
        let tasks: Vec<Task> = serde_json::from_str(json).unwrap();
        assert_eq!(tasks[0].priority, 0);
        assert_eq!(tasks[0].due, None);
        // Defaults are not written back.
        let out = serde_json::to_string(&tasks).unwrap();
        assert_eq!(out, r#"[{"id":1,"description":"old","done":true}]"#);
    }

    #[test]
    fn due_labels() {
        let t = today();
        assert_eq!(format_due(t, t), "today");
        assert_eq!(format_due(d(2026, 10, 23), t), "tomorrow");
        assert_eq!(format_due(d(2026, 10, 21), t), "yesterday");
        assert_eq!(format_due(d(2026, 10, 26), t), "mon");
        assert_eq!(format_due(d(2026, 11, 1), t), "nov 01");
        assert_eq!(format_due(d(2027, 1, 5), t), "2027-01-05");
        assert_eq!(urgency(d(2026, 10, 1), t), Urgency::Overdue);
        assert_eq!(urgency(t, t), Urgency::Today);
    }
}
