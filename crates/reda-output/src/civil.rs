//! Calendar dates for the time stamps outputs carry (Gerber `.CreationDate`, zip entries).

use reda_command::Timestamp;

/// A UTC calendar date and time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Civil {
    pub year: i64,
    pub month: i64,
    pub day: i64,
    pub hour: i64,
    pub minute: i64,
    pub second: i64,
}

impl Civil {
    /// `at` (ms since the Unix epoch) in UTC, to the second.
    pub(crate) fn of(at: Timestamp) -> Civil {
        let secs = at.0.div_euclid(1000);
        let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
        // Howard Hinnant's days-to-civil algorithm.
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = doy - (153 * mp + 2) / 5 + 1;
        let month = if mp < 10 { mp + 3 } else { mp - 9 };
        Civil {
            year: yoe + era * 400 + i64::from(month <= 2),
            month,
            day,
            hour: rem / 3600,
            minute: rem % 3600 / 60,
            second: rem % 60,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_stamps_are_utc_calendar_dates() {
        let c = Civil::of(Timestamp(951_782_400_000 + 3_723_000));
        assert_eq!(
            (c.year, c.month, c.day, c.hour, c.minute, c.second),
            (2000, 2, 29, 1, 2, 3)
        );
        let c = Civil::of(Timestamp(-1000));
        assert_eq!((c.year, c.month, c.day, c.second), (1969, 12, 31, 59));
    }
}
