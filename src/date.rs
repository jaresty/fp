/// Subtract one second from a git `--format=%ci` date string (`YYYY-MM-DD HH:MM:SS +ZZZZ`).
/// Returns None if the string cannot be parsed.
pub fn subtract_one_second(date: &str) -> Option<String> {
    let parts: Vec<&str> = date.splitn(3, ' ').collect();
    if parts.len() != 3 { return None; }
    let ymd: Vec<&str> = parts[0].splitn(3, '-').collect();
    let hms: Vec<&str> = parts[1].splitn(3, ':').collect();
    if ymd.len() != 3 || hms.len() != 3 { return None; }
    let year: i32 = ymd[0].parse().ok()?;
    let month: u8 = ymd[1].parse().ok()?;
    let day: u8 = ymd[2].parse().ok()?;
    let hour: u8 = hms[0].parse().ok()?;
    let min: u8 = hms[1].parse().ok()?;
    let sec: u8 = hms[2].parse().ok()?;
    let tz = parts[2];
    let (new_year, new_month, new_day, new_hour, new_min, new_sec) =
        if sec > 0 {
            (year, month, day, hour, min, sec - 1)
        } else if min > 0 {
            (year, month, day, hour, min - 1, 59)
        } else if hour > 0 {
            (year, month, day, hour - 1, 59, 59)
        } else {
            // Don't bother with day rollback — just return same date at 00:00:00
            (year, month, day, 0, 0, 0)
        };
    Some(format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02} {}", new_year, new_month, new_day, new_hour, new_min, new_sec, tz))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subtract_one_second_decrements_seconds() {
        assert_eq!(
            subtract_one_second("2024-01-15 10:30:45 +0000"),
            Some("2024-01-15 10:30:44 +0000".to_string())
        );
    }

    #[test]
    fn subtract_one_second_rolls_over_minutes() {
        assert_eq!(
            subtract_one_second("2024-01-15 10:30:00 +0000"),
            Some("2024-01-15 10:29:59 +0000".to_string())
        );
    }

    #[test]
    fn subtract_one_second_returns_none_on_bad_input() {
        assert_eq!(subtract_one_second("not-a-date"), None);
    }
}
