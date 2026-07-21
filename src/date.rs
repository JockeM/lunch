use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Date {
    pub iso_year: i32,
    pub iso_week: u8,
    pub weekday: Weekday,
}

impl Date {
    pub fn today_utc() -> Self {
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time must be after Unix epoch")
            .as_secs();
        let days = i64::try_from(seconds / 86_400).expect("Unix day count must fit in i64");

        Self::from_days_since_unix_epoch(days)
    }

    fn from_days_since_unix_epoch(days: i64) -> Self {
        let weekday = Weekday::from_days_since_unix_epoch(days);
        // An ISO week belongs to the year containing that week's Thursday.
        let thursday = days + i64::from(4 - weekday.iso_number());
        let iso_year = civil_year(thursday);
        let january_fourth = days_from_civil(iso_year, 1, 4);
        let week_one_monday = january_fourth
            - i64::from(Weekday::from_days_since_unix_epoch(january_fourth).iso_number() - 1);
        let iso_week =
            u8::try_from((days - week_one_monday) / 7 + 1).expect("ISO week number must fit in u8");

        Self {
            iso_year,
            iso_week,
            weekday,
        }
    }
}

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

    const fn iso_number(self) -> i8 {
        match self {
            Self::Monday => 1,
            Self::Tuesday => 2,
            Self::Wednesday => 3,
            Self::Thursday => 4,
            Self::Friday => 5,
            Self::Saturday => 6,
            Self::Sunday => 7,
        }
    }
}

fn civil_year(days: i64) -> i32 {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;

    if month_prime >= 10 {
        year += 1;
    }

    i32::try_from(year).expect("civil year must fit in i32")
}

fn days_from_civil(year: i32, month: u8, day: u8) -> i64 {
    let mut year = i64::from(year);
    let month = i64::from(month);
    let day = i64::from(day);

    year -= i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month_prime = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * month_prime + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;

    era * 146_097 + day_of_era - 719_468
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

    #[test]
    fn maps_iso_week_across_year_boundary() {
        assert_eq!(
            Date::from_days_since_unix_epoch(days_from_civil(2021, 1, 1)),
            Date {
                iso_year: 2020,
                iso_week: 53,
                weekday: Weekday::Friday,
            }
        );
        assert_eq!(
            Date::from_days_since_unix_epoch(days_from_civil(2026, 7, 21)),
            Date {
                iso_year: 2026,
                iso_week: 30,
                weekday: Weekday::Tuesday,
            }
        );
    }
}
