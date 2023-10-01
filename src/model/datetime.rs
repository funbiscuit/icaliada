use std::collections::HashMap;

use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, NaiveDateTime, TimeZone, Utc};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DatePerhapsTime {
    Date(NaiveDate),
    DateTime(DateTime<Utc>),
}

impl DatePerhapsTime {
    pub fn into_datetime(self) -> DateTime<Utc> {
        match self {
            DatePerhapsTime::DateTime(start) => start,
            DatePerhapsTime::Date(start) => Utc
                .from_local_datetime(&start.and_hms_opt(0, 0, 0).unwrap())
                .unwrap(),
        }
    }

    pub fn is_date(&self) -> bool {
        match self {
            DatePerhapsTime::Date(_) => true,
            DatePerhapsTime::DateTime(_) => false,
        }
    }

    pub fn new(
        value: String,
        props: Vec<(String, Vec<String>)>,
        local_to_utc: impl Fn(&str, NaiveDateTime) -> DateTime<Utc>,
    ) -> Result<DatePerhapsTime> {
        let props: HashMap<_, _> = props.into_iter().collect();

        let is_date = props
            .get("VALUE")
            .map(|v| v.iter().any(|f| f == "DATE"))
            .unwrap_or(false);

        if is_date {
            let date = convert_date(value)?;
            Ok(DatePerhapsTime::Date(date))
        } else {
            let datetime = convert_datetime(value, props, &local_to_utc)?;
            Ok(DatePerhapsTime::DateTime(datetime))
        }
    }
}

/// In time range both start and end must be the same variant
/// (either both date or both datetime)
#[derive(Clone, Debug)]
pub struct TimeRange {
    start: DatePerhapsTime,
    end: DatePerhapsTime,
}

impl TimeRange {
    pub fn either<T>(
        &self,
        f: impl FnOnce(NaiveDate, NaiveDate) -> T,
        g: impl FnOnce(DateTime<Utc>, DateTime<Utc>) -> T,
    ) -> T {
        match (self.start, self.end) {
            (DatePerhapsTime::Date(s), DatePerhapsTime::Date(e)) => f(s, e),
            (DatePerhapsTime::DateTime(s), DatePerhapsTime::DateTime(e)) => g(s, e),
            _ => unreachable!(),
        }
    }

    pub fn intersects(&self, start: &DateTime<Utc>, end: &DateTime<Utc>) -> bool {
        match (&self.start, &self.end) {
            (DatePerhapsTime::DateTime(s), DatePerhapsTime::DateTime(e)) => end >= s && start <= e,
            (DatePerhapsTime::Date(s), DatePerhapsTime::Date(e)) => {
                &end.date_naive() >= s && &start.date_naive() <= e
            }
            _ => unreachable!(),
        }
    }

    pub fn is_all_day(&self) -> bool {
        match self.start {
            DatePerhapsTime::Date(_) => true,
            DatePerhapsTime::DateTime(_) => false,
        }
    }

    pub fn new(
        start: String,
        start_props: Vec<(String, Vec<String>)>,
        end: String,
        end_props: Vec<(String, Vec<String>)>,
        local_to_utc: impl Fn(&str, NaiveDateTime) -> DateTime<Utc>,
    ) -> Result<Self> {
        let start = DatePerhapsTime::new(start, start_props, &local_to_utc)?;
        let end = DatePerhapsTime::new(end, end_props, &local_to_utc)?;

        anyhow::ensure!(
            start.is_date() == end.is_date(),
            "Start and end must have equal types"
        );

        Ok(Self { start, end })
    }

    pub fn start(&self) -> DatePerhapsTime {
        self.start
    }

    pub fn with_start(&self, new_start: DateTime<Utc>) -> Self {
        let new_start_date = new_start.date_naive();

        match (self.start, self.end) {
            (DatePerhapsTime::DateTime(start), DatePerhapsTime::DateTime(end)) => Self {
                start: DatePerhapsTime::DateTime(new_start),
                end: DatePerhapsTime::DateTime(new_start + (end - start)),
            },
            (DatePerhapsTime::Date(start), DatePerhapsTime::Date(end)) => Self {
                start: DatePerhapsTime::Date(new_start_date),
                end: DatePerhapsTime::Date(new_start_date + (end - start)),
            },
            _ => unreachable!(),
        }
    }
}

fn convert_datetime(
    value: String,
    properties: HashMap<String, Vec<String>>,
    local_to_utc: impl Fn(&str, NaiveDateTime) -> DateTime<Utc>,
) -> Result<DateTime<Utc>> {
    let fmt = "%Y%m%dT%H%M%S";
    if value.ends_with('Z') {
        let time = NaiveDateTime::parse_from_str(&value[..value.len() - 1], fmt)
            .context(format!("Failed to convert datetime: {}", value))?;

        let time = Utc
            .from_local_datetime(&time)
            .earliest()
            .context("Failed to convert time")?;
        Ok(time)
    } else {
        let time = NaiveDateTime::parse_from_str(&value, fmt)
            .context(format!("Failed to convert datetime: {}", value))?;

        let prop = properties
            .get("TZID")
            .context("Missing TZID for datetime")?;
        anyhow::ensure!(prop.len() == 1, "TZID must be set only once");
        let timezone = &prop[0];

        Ok(local_to_utc(timezone, time))
    }
}

fn convert_date(value: String) -> Result<NaiveDate> {
    let fmt = "%Y%m%d";
    NaiveDate::parse_from_str(&value, fmt).context(format!("Failed to convert date: {}", value))
}
