use chrono::{Datelike, Duration, NaiveDate, Weekday};
use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum PeriodKind {
    Daily,
    Weekly,
    Monthly,
    Quarterly,
    Yearly,
}

impl PeriodKind {
    pub const ALL: [Self; 5] = [
        Self::Daily,
        Self::Weekly,
        Self::Monthly,
        Self::Quarterly,
        Self::Yearly,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Daily => "daily",
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
            Self::Quarterly => "quarterly",
            Self::Yearly => "yearly",
        }
    }

    pub fn current_key(&self, date: NaiveDate) -> String {
        match self {
            Self::Daily => date.format("%Y-%m-%d").to_string(),
            Self::Weekly => date.format("%G-W%V").to_string(),
            Self::Monthly => date.format("%Y-%m").to_string(),
            Self::Quarterly => {
                let quarter = ((date.month() - 1) / 3) + 1;
                format!("{}-Q{quarter}", date.year())
            }
            Self::Yearly => date.format("%Y").to_string(),
        }
    }

    pub fn period_bounds(&self, date: NaiveDate) -> (NaiveDate, NaiveDate) {
        match self {
            Self::Daily => (date, date),
            Self::Weekly => {
                let start = date - Duration::days(date.weekday().num_days_from_monday() as i64);
                (start, start + Duration::days(6))
            }
            Self::Monthly => {
                let start = NaiveDate::from_ymd_opt(date.year(), date.month(), 1).unwrap();
                let end = last_day_of_month(date.year(), date.month());
                (start, end)
            }
            Self::Quarterly => {
                let start_month = ((date.month() - 1) / 3) * 3 + 1;
                let end_month = start_month + 2;
                let start = NaiveDate::from_ymd_opt(date.year(), start_month, 1).unwrap();
                let end = last_day_of_month(date.year(), end_month);
                (start, end)
            }
            Self::Yearly => (
                NaiveDate::from_ymd_opt(date.year(), 1, 1).unwrap(),
                NaiveDate::from_ymd_opt(date.year(), 12, 31).unwrap(),
            ),
        }
    }

    pub fn bounds_for_key(&self, key: &str) -> Option<(NaiveDate, NaiveDate)> {
        let date = match self {
            Self::Daily => NaiveDate::parse_from_str(key, "%Y-%m-%d").ok()?,
            Self::Weekly => parse_iso_week_start(key)?,
            Self::Monthly => {
                let (year, month) = key.split_once('-')?;
                NaiveDate::from_ymd_opt(year.parse().ok()?, month.parse().ok()?, 1)?
            }
            Self::Quarterly => {
                let (year, quarter) = key.split_once("-Q")?;
                let start_month = (quarter.parse::<u32>().ok()?.checked_sub(1)? * 3) + 1;
                NaiveDate::from_ymd_opt(year.parse().ok()?, start_month, 1)?
            }
            Self::Yearly => NaiveDate::from_ymd_opt(key.parse().ok()?, 1, 1)?,
        };
        Some(self.period_bounds(date))
    }
}

impl std::fmt::Display for PeriodKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for PeriodKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "daily" => Ok(Self::Daily),
            "weekly" => Ok(Self::Weekly),
            "monthly" => Ok(Self::Monthly),
            "quarterly" => Ok(Self::Quarterly),
            "yearly" => Ok(Self::Yearly),
            _ => Err(format!("unknown period kind: {s}")),
        }
    }
}

fn parse_iso_week_start(key: &str) -> Option<NaiveDate> {
    let (year, week) = key.split_once("-W")?;
    NaiveDate::from_isoywd_opt(year.parse().ok()?, week.parse().ok()?, Weekday::Mon)
}

fn last_day_of_month(year: i32, month: u32) -> NaiveDate {
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    NaiveDate::from_ymd_opt(next_year, next_month, 1).unwrap() - Duration::days(1)
}

#[cfg(test)]
mod tests {
    use super::PeriodKind;
    use chrono::NaiveDate;

    #[test]
    fn current_key_formats_all_period_kinds() {
        let date = NaiveDate::from_ymd_opt(2026, 5, 23).unwrap();

        assert_eq!(PeriodKind::Daily.current_key(date), "2026-05-23");
        assert_eq!(PeriodKind::Weekly.current_key(date), "2026-W21");
        assert_eq!(PeriodKind::Monthly.current_key(date), "2026-05");
        assert_eq!(PeriodKind::Quarterly.current_key(date), "2026-Q2");
        assert_eq!(PeriodKind::Yearly.current_key(date), "2026");
    }

    #[test]
    fn period_bounds_cover_year_and_quarter_edges() {
        let jan_first = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        assert_eq!(
            PeriodKind::Weekly.period_bounds(jan_first),
            (
                NaiveDate::from_ymd_opt(2025, 12, 29).unwrap(),
                NaiveDate::from_ymd_opt(2026, 1, 4).unwrap()
            )
        );

        let quarter_end = NaiveDate::from_ymd_opt(2026, 12, 31).unwrap();
        assert_eq!(
            PeriodKind::Quarterly.period_bounds(quarter_end),
            (
                NaiveDate::from_ymd_opt(2026, 10, 1).unwrap(),
                NaiveDate::from_ymd_opt(2026, 12, 31).unwrap()
            )
        );

        let leap_feb = NaiveDate::from_ymd_opt(2024, 2, 14).unwrap();
        assert_eq!(
            PeriodKind::Monthly.period_bounds(leap_feb),
            (
                NaiveDate::from_ymd_opt(2024, 2, 1).unwrap(),
                NaiveDate::from_ymd_opt(2024, 2, 29).unwrap()
            )
        );
    }

    #[test]
    fn bounds_for_key_parses_all_period_kinds() {
        assert_eq!(
            PeriodKind::Daily.bounds_for_key("2026-05-23"),
            Some((
                NaiveDate::from_ymd_opt(2026, 5, 23).unwrap(),
                NaiveDate::from_ymd_opt(2026, 5, 23).unwrap()
            ))
        );
        assert_eq!(
            PeriodKind::Weekly.bounds_for_key("2026-W21"),
            Some((
                NaiveDate::from_ymd_opt(2026, 5, 18).unwrap(),
                NaiveDate::from_ymd_opt(2026, 5, 24).unwrap()
            ))
        );
        assert_eq!(
            PeriodKind::Monthly.bounds_for_key("2026-05"),
            Some((
                NaiveDate::from_ymd_opt(2026, 5, 1).unwrap(),
                NaiveDate::from_ymd_opt(2026, 5, 31).unwrap()
            ))
        );
        assert_eq!(
            PeriodKind::Quarterly.bounds_for_key("2026-Q2"),
            Some((
                NaiveDate::from_ymd_opt(2026, 4, 1).unwrap(),
                NaiveDate::from_ymd_opt(2026, 6, 30).unwrap()
            ))
        );
        assert_eq!(
            PeriodKind::Yearly.bounds_for_key("2026"),
            Some((
                NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                NaiveDate::from_ymd_opt(2026, 12, 31).unwrap()
            ))
        );
    }
}
