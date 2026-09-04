//! One ayah and one narration, chosen by the date.
//!
//! SELECTION IS NOT AUTHORSHIP, and the difference is that this is a pure
//! function of the day: the same date picks the same pair on every machine, for
//! everyone, and there is nothing here that could produce text. It is a rotation
//! through datasets that are already installed and already show their sources.
//!
//! The caller passes its LOCAL date. The crate has no timezone database and
//! deriving the day from the clock would roll the ayah over at six in the
//! morning in Dhaka, which is a strange time for a day to start.

use anyhow::{anyhow, Result};

/// Days from 1970-01-01 to a proleptic Gregorian y-m-d.
///
/// Howard Hinnant's `days_from_civil`, which is the standard way to do this
/// without a calendar library: shift the year to start in March so the leap day
/// lands at the end of it, and the month lengths become a arithmetic sequence.
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as i64; // [0, 399]
    let mp = ((m + 9) % 12) as i64; // March = 0
    let doy = (153 * mp + 2) / 5 + d as i64 - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe - 719_468
}

/// Parse `YYYY-MM-DD` into days since the epoch.
pub fn day_number(date: &str) -> Result<i64> {
    let bad = || anyhow!("expected a date like 2026-09-05, got {date:?}");
    let mut parts = date.trim().split('-');
    let y: i64 = parts.next().ok_or_else(bad)?.parse().map_err(|_| bad())?;
    let m: u32 = parts.next().ok_or_else(bad)?.parse().map_err(|_| bad())?;
    let d: u32 = parts.next().ok_or_else(bad)?.parse().map_err(|_| bad())?;
    if parts.next().is_some() || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return Err(bad());
    }
    Ok(days_from_civil(y, m, d))
}

/// Today in UTC, as `YYYY-MM-DD`. The fallback when no date is given, and wrong
/// by up to a day's edge for anyone who does not pass one.
pub fn utc_today() -> String {
    let days = (crate::hifz::now() / 86_400) as i64;
    // Invert days_from_civil. Same source, same trick.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

/// Which item of `count` today is.
///
/// SPREAD, NOT WALKED. A plain `day % count` gives consecutive days consecutive
/// ayat, which means 286 days in a row inside Al-Baqarah and twenty years to
/// leave Bukhari — a page that changes but does not move. Multiplying by
/// Knuth's constant first scatters the days across the range while keeping this
/// a pure function of the date, so every machine still shows the same ayah.
pub fn index_for(day: i64, count: usize) -> usize {
    if count == 0 {
        return 0;
    }
    (i128::from(day) * 2_654_435_761).rem_euclid(count as i128) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_days_since_the_epoch() {
        assert_eq!(day_number("1970-01-01").unwrap(), 0);
        assert_eq!(day_number("2026-09-05").unwrap(), 20701);
        // A day apart is one apart, across a month and a leap year.
        assert_eq!(
            day_number("2028-03-01").unwrap() - day_number("2028-02-29").unwrap(),
            1
        );
        assert!(day_number("2026-13-01").is_err());
        assert!(day_number("nope").is_err());
    }

    #[test]
    fn the_two_directions_agree() {
        // utc_today inverts days_from_civil, so a round trip has to hold.
        let today = utc_today();
        let n = day_number(&today).unwrap();
        assert_eq!((crate::hifz::now() / 86_400) as i64, n);
    }

    #[test]
    fn selection_is_stable_and_moves_once_a_day() {
        let a = day_number("2026-09-05").unwrap();
        let b = day_number("2026-09-06").unwrap();
        assert_eq!(index_for(a, 6236), index_for(a, 6236));
        assert_ne!(index_for(a, 6236), index_for(b, 6236));
        // And not merely one apart: a page that advances by a single ayah a day
        // never leaves the surah it started in.
        assert!((index_for(a, 6236) as i64 - index_for(b, 6236) as i64).abs() > 1);
        assert!(index_for(a, 6236) < 6236);
        // A date before the epoch must not index backwards off the array.
        assert!(index_for(day_number("1969-01-01").unwrap(), 100) < 100);
    }
}
