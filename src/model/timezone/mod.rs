use anyhow::Context;
use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeZone, Utc};
use ical::parser::ical::component::{IcalTimeZone, IcalTimeZoneTransition};
use rrule::{RRule, RRuleSet, Tz};
use std::str::FromStr;

use crate::service::utils;

#[derive(Clone, Debug)]
pub struct Timezone {
    id: String,
    transitions: Vec<TimezoneTransition>,
}

#[derive(Clone, Debug)]
pub struct TimezoneTransition {
    rule: RRuleSet,
    from: FixedOffset,
    to: FixedOffset,
}

impl Timezone {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn local_to_utc(&self, datetime: NaiveDateTime) -> DateTime<Utc> {
        //rrule crate works only with timezoned dates, so assume local as utc
        let datetime = Tz::UTC.from_local_datetime(&datetime).unwrap();

        let mut last_transition_time = None;
        let mut last_transition_offset = None;

        for transition in &self.transitions {
            let last = transition
                .rule
                .clone()
                .limit()
                .into_iter()
                .take_while(|d| d <= &datetime)
                .last();

            if let Some(last) = last {
                if let Some(last_tr) = last_transition_time {
                    if last > last_tr {
                        last_transition_time = Some(last);
                        last_transition_offset = Some(transition.to);
                    }
                } else {
                    last_transition_time = Some(last);
                    last_transition_offset = Some(transition.to);
                }
            }
        }

        let offset = if let Some(offset) = last_transition_offset {
            offset
        } else {
            // date is before all transitions, so find first transition and take its offset_from
            //todo precompute
            let mut first_transition_time = None;
            let mut first_transition_offset = None;

            for transition in &self.transitions {
                let first = transition.rule.clone().into_iter().next();

                if let Some(first) = first {
                    if let Some(first_tr) = first_transition_time {
                        if first < first_tr {
                            first_transition_time = Some(first);
                            first_transition_offset = Some(transition.from);
                        }
                    } else {
                        first_transition_time = Some(first);
                        first_transition_offset = Some(transition.from);
                    }
                }
            }

            first_transition_offset.unwrap()
        };

        let new_date = offset.from_local_datetime(&datetime.naive_utc()).unwrap();

        new_date.with_timezone(&Utc)
    }
}

impl TryFrom<IcalTimeZone> for Timezone {
    type Error = anyhow::Error;

    fn try_from(cal_tz: IcalTimeZone) -> Result<Self, Self::Error> {
        let id = cal_tz
            .properties
            .into_iter()
            .find(|p| p.name == "TZID")
            .and_then(|p| p.value)
            .map(utils::unescape)
            .context("Timezone ID is missing")?;

        let mut transitions = vec![];

        for cal_trans in cal_tz.transitions {
            transitions.push(parse_transition(cal_trans));
        }

        let timezone = Timezone { id, transitions };

        Ok(timezone)
    }
}

fn parse_transition(cal_transition: IcalTimeZoneTransition) -> TimezoneTransition {
    let mut from = None;
    let mut to = None;

    let mut dtstart = None;
    let mut occurences = vec![];
    let fmt = "%Y%m%dT%H%M%S";

    let mut rrule = None;

    for props in cal_transition.properties {
        match props.name.as_str() {
            "TZOFFSETFROM" => {
                let offset: Option<FixedOffset> = props.value.unwrap().parse().ok();
                from = offset;
            }
            "TZOFFSETTO" => {
                let offset: Option<FixedOffset> = props.value.unwrap().parse().ok();
                to = offset;
            }
            "RRULE" => {
                rrule = Some(RRule::from_str(&props.value.unwrap()).unwrap());
            }
            "DTSTART" => {
                let occurence = props
                    .value
                    .and_then(|t| NaiveDateTime::parse_from_str(&t, fmt).ok())
                    .unwrap();
                dtstart = Some(occurence);
                occurences.push(occurence);
            }
            "RDATE" => {
                let occurence = props
                    .value
                    .and_then(|t| NaiveDateTime::parse_from_str(&t, fmt).ok())
                    .unwrap();
                occurences.push(occurence);
            }
            _ => {}
        }
    }

    let from = from.unwrap();
    let to = to.unwrap();

    let dtsart = dtstart.unwrap();
    // we store all local datetimes with UTC timezone (which is actually incorrect)
    // but that's the only option with rrule since it doesn't support arbitrary timezones
    let dtstart = Tz::UTC.from_local_datetime(&dtsart).unwrap();

    let mut rule = RRuleSet::new(dtstart);

    if let Some(mut rrule) = rrule {
        if let Some(until) = rrule.get_until() {
            let until = Tz::UTC
                .from_local_datetime(&until.with_timezone(&from).naive_local())
                .unwrap();

            rrule = rrule.until(until);
        }

        let rrule = rrule.validate(dtstart).unwrap();

        rule = rule.rrule(rrule);
    } else {
        let occurences = occurences
            .into_iter()
            .map(|o| Tz::UTC.from_local_datetime(&o).unwrap())
            .collect();

        rule = rule.set_rdates(occurences);
    }

    TimezoneTransition { rule, from, to }
}

#[cfg(test)]
mod tests {
    use crate::model::Timezone;
    use chrono::{DateTime, NaiveDateTime, Utc};
    use rstest::rstest;

    #[rstest]
    #[case("1967-04-15T00:00:00", "1967-04-15T05:00:00Z")]
    #[case("1967-05-15T00:00:00", "1967-05-15T04:00:00Z")]
    #[case("1980-04-26T00:00:00", "1980-04-26T05:00:00Z")]
    #[case("1980-04-28T00:00:00", "1980-04-28T04:00:00Z")]
    #[case("1980-10-25T00:00:00", "1980-10-25T04:00:00Z")]
    #[case("1980-10-27T00:00:00", "1980-10-27T05:00:00Z")]
    #[case("2000-12-20T00:00:00", "2000-12-20T05:00:00Z")]
    #[case("2010-03-13T00:00:00", "2010-03-13T05:00:00Z")]
    #[case("2010-03-15T00:00:00", "2010-03-15T04:00:00Z")]
    #[case("2010-11-06T00:00:00", "2010-11-06T04:00:00Z")]
    #[case("2010-11-08T00:00:00", "2010-11-08T05:00:00Z")]
    fn test_new_york(#[case] addr: &str, #[case] expected: &str) {
        let bytes = include_bytes!("test-tz-new-york.ics");
        test_date_conversion(bytes, addr, expected);
    }

    #[rstest]
    #[case("2010-10-30T02:00:00", "2010-10-29T22:00:00Z")]
    #[case("2010-11-01T02:00:00", "2010-10-31T23:00:00Z")]
    #[case("2015-11-01T02:00:00", "2015-10-31T23:00:00Z")]
    fn test_moscow(#[case] addr: &str, #[case] expected: &str) {
        let bytes = include_bytes!("test-tz-moscow.ics");
        test_date_conversion(bytes, addr, expected);
    }

    fn test_date_conversion(ical_bytes: &[u8], local_date: &str, expected_date: &str) {
        let local_date = NaiveDateTime::parse_from_str(local_date, "%Y-%m-%dT%H:%M:%S").unwrap();
        let expected_date = DateTime::parse_from_rfc3339(expected_date)
            .unwrap()
            .with_timezone(&Utc);

        let reader = ical::IcalParser::new(ical_bytes);
        let calendar = reader.flatten().next().unwrap();
        let cal_tz = calendar.timezones.into_iter().next().unwrap();

        let timezone = Timezone::try_from(cal_tz).unwrap();
        let date = timezone.local_to_utc(local_date);

        assert_eq!(date, expected_date);
    }
}
