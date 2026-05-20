use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Weekday {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

impl Weekday {
    pub fn today_utc() -> Self {
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time must be after Unix epoch")
            .as_secs();
        let days = i64::try_from(seconds / 86_400).expect("Unix day count must fit in i64");

        Self::from_days_since_unix_epoch(days)
    }

    fn from_days_since_unix_epoch(days: i64) -> Self {
        match (days + 3).rem_euclid(7) {
            0 => Self::Monday,
            1 => Self::Tuesday,
            2 => Self::Wednesday,
            3 => Self::Thursday,
            4 => Self::Friday,
            5 => Self::Saturday,
            6 => Self::Sunday,
            _ => unreachable!("weekday modulo must stay in 0..=6"),
        }
    }
}

impl fmt::Display for Weekday {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Monday => "Monday",
            Self::Tuesday => "Tuesday",
            Self::Wednesday => "Wednesday",
            Self::Thursday => "Thursday",
            Self::Friday => "Friday",
            Self::Saturday => "Saturday",
            Self::Sunday => "Sunday",
        };

        f.write_str(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_weekday_from_unix_days() {
        assert_eq!(Weekday::from_days_since_unix_epoch(0), Weekday::Thursday);
        assert_eq!(
            Weekday::from_days_since_unix_epoch(20_593),
            Weekday::Wednesday
        );
    }
}
