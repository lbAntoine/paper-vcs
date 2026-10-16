//! Relative times for `paper history`.

use jiff::Timestamp;
use jiff::tz::TimeZone;

/// Renders `then` relative to `now` (both seconds since the Unix epoch, UTC).
///
/// - under a minute (or in the future, from clock skew): `just now`
/// - under an hour: `1 minute ago`, `N minutes ago`
/// - under a day: `1 hour ago`, `N hours ago`
/// - a day to under two days: `yesterday`
/// - older: the UTC date, `YYYY-MM-DD`
pub fn relative_time(then: i64, now: i64) -> String {
    let elapsed = now.saturating_sub(then);
    let minutes = elapsed / 60;
    let hours = elapsed / 3600;
    match elapsed {
        ..60 => "just now".to_owned(),
        60..120 => "1 minute ago".to_owned(),
        120..3600 => format!("{minutes} minutes ago"),
        3600..7200 => "1 hour ago".to_owned(),
        7200..86_400 => format!("{hours} hours ago"),
        86_400..172_800 => "yesterday".to_owned(),
        _ => utc_date(then),
    }
}

/// `YYYY-MM-DD` in UTC; the raw seconds if the time is outside what dates can show.
fn utc_date(seconds: i64) -> String {
    Timestamp::from_second(seconds).map_or_else(
        |_| seconds.to_string(),
        |ts| ts.to_zoned(TimeZone::UTC).strftime("%Y-%m-%d").to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::relative_time;

    // 2026-10-16T12:00:00Z
    const NOW: i64 = 1_792_152_000;

    fn ago(seconds: i64) -> String {
        relative_time(NOW - seconds, NOW)
    }

    #[test]
    fn the_same_second_is_just_now() {
        assert_eq!(ago(0), "just now");
    }

    #[test]
    fn under_a_minute_is_just_now() {
        assert_eq!(ago(59), "just now");
    }

    #[test]
    fn a_time_in_the_future_is_just_now() {
        assert_eq!(relative_time(NOW + 300, NOW), "just now");
    }

    #[test]
    fn one_minute_is_singular() {
        assert_eq!(ago(60), "1 minute ago");
    }

    #[test]
    fn minutes_are_counted_and_rounded_down() {
        assert_eq!(ago(10 * 60 + 59), "10 minutes ago");
        assert_eq!(ago(59 * 60 + 59), "59 minutes ago");
    }

    #[test]
    fn one_hour_is_singular() {
        assert_eq!(ago(3600), "1 hour ago");
    }

    #[test]
    fn hours_are_counted_and_rounded_down() {
        assert_eq!(ago(2 * 3600 + 1800), "2 hours ago");
        assert_eq!(ago(23 * 3600 + 3599), "23 hours ago");
    }

    #[test]
    fn a_day_to_two_days_ago_is_yesterday() {
        assert_eq!(ago(24 * 3600), "yesterday");
        assert_eq!(ago(48 * 3600 - 1), "yesterday");
    }

    #[test]
    fn two_days_or_more_is_the_utc_date() {
        // 2026-10-14T12:00:00Z
        assert_eq!(ago(48 * 3600), "2026-10-14");
        // 2026-09-10T12:00:00Z
        assert_eq!(ago(36 * 24 * 3600), "2026-09-10");
    }

    #[test]
    fn the_date_is_in_utc_not_local_time() {
        // 2026-10-13T23:59:59Z, long enough ago to be a date
        let then = NOW - (2 * 24 * 3600 + 12 * 3600 + 1);
        assert_eq!(relative_time(then, NOW), "2026-10-13");
    }
}
