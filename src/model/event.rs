use std::collections::HashMap;
use std::str::FromStr;

use crate::model::datetime::{DatePerhapsTime, TimeRange};
use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;
use ical::parser::ical::component::IcalEvent;
use rrule::{RRule, RRuleSet};

#[derive(Clone, Debug)]
pub struct CalendarEvent {
    pub range: TimeRange,
    pub summary: String,
    pub recurrence: Option<RRuleSet>,
    pub recurrence_id: Option<DatePerhapsTime>,
    pub uid: String,
}

impl CalendarEvent {
    pub fn from_ical_event(
        value: IcalEvent,
        local_to_utc: impl Fn(&str, NaiveDateTime) -> DateTime<Utc>,
    ) -> Result<Self> {
        // todo proper errors
        let mut props: HashMap<_, _> = value
            .properties
            .into_iter()
            .map(|prop| (prop.name, (prop.value, prop.params.unwrap_or_default())))
            .collect();

        let (uid, _) = props.remove("UID").context("UID is missing")?;
        let uid = uid.context("UID is missing")?;
        let (start_value, start_props) = props.remove("DTSTART").context("DTSTART is missing")?;
        let start_value = start_value.context("DTSTART is missing")?;
        let (end_value, end_props) = props.remove("DTEND").context("DTEND is missing")?;
        let end_value = end_value.context("DTEND is missing")?;
        let recurrence_id = props.remove("RECURRENCE-ID");
        let (summary, _) = props.remove("SUMMARY").context("SUMMARY is missing")?;
        let summary = summary.context("SUMMARY is missing")?;
        let rrule = props
            .remove("RRULE")
            .and_then(|(v, _)| v)
            .and_then(|rrule| RRule::from_str(&rrule).ok());

        let range = TimeRange::new(
            start_value,
            start_props,
            end_value,
            end_props,
            &local_to_utc,
        )?;

        let recurrence_id = recurrence_id
            .map(|(value, props)| DatePerhapsTime::new(value.unwrap(), props, &local_to_utc))
            .transpose()?;

        let start = range.start().into_datetime();

        let dtstart = start.with_timezone(&rrule::Tz::Tz(Tz::UTC));
        let recurrence = rrule
            .and_then(|mut rrule| {
                // when range is all day, manually change until from local to utc
                // since we use utc for start
                if range.is_all_day() {
                    if let Some(until) = rrule.get_until() {
                        let until = rrule::Tz::Tz(Tz::UTC)
                            .from_local_datetime(&until.date_naive().and_hms_opt(0, 0, 0).unwrap())
                            .unwrap();
                        rrule = rrule.until(until);
                    }
                }

                rrule
                    .validate(dtstart)
                    .map_err(|e| {
                        tracing::error!("Failed to validate rrule: {:?}", e);
                    })
                    .ok()
            })
            .map(|rrule| RRuleSet::new(dtstart).rrule(rrule));

        Ok(Self {
            range,
            summary,
            recurrence,
            recurrence_id,
            uid,
        })
    }
}

#[derive(Clone, Debug)]
pub struct PrimitiveEvent {
    pub range: TimeRange,
    pub summary: String,
}

#[derive(Clone, Debug)]
pub struct EventOverride {
    pub range: TimeRange,
    pub summary: String,
    pub recurrence_id: DatePerhapsTime,
}

#[derive(Clone, Debug)]
pub struct EventSet {
    pub uid: String,
    pub range: TimeRange,
    pub summary: String,
    pub recurrence: Option<RRuleSet>,

    /// All overrides of normal recurrence set.
    /// It is not empty only if recurrence is not None
    pub overrides: Vec<EventOverride>,
}

impl EventSet {
    /// Creates initial event set from recurrence rule (if specified).
    /// Otherwise returns single event
    fn create_initial_events(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Vec<PrimitiveEvent> {
        match &self.recurrence {
            Some(rule) => {
                let start = start.with_timezone(&rrule::Tz::Tz(Tz::UTC));
                let end = end.with_timezone(&rrule::Tz::Tz(Tz::UTC));
                // limited not checked
                let result = rule.clone().after(start).before(end).all(100);

                if result.limited {
                    tracing::warn!("RRule expansion gave more than 100 results!")
                }

                result
                    .dates
                    .into_iter()
                    .map(|s| s.with_timezone(&Utc))
                    .map(|start| PrimitiveEvent {
                        range: self.range.with_start(start),
                        summary: self.summary.clone(),
                    })
                    .collect()
            }
            None => {
                if self.range.intersects(&start, &end) {
                    vec![PrimitiveEvent {
                        range: self.range.clone(),
                        summary: self.summary.clone(),
                    }]
                } else {
                    vec![]
                }
            }
        }
    }

    /// Creates list of primitive events for this event set
    pub fn create_primitives(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Vec<PrimitiveEvent> {
        let initial = self.create_initial_events(start, end);

        initial
            .into_iter()
            .map(|event| {
                let event_override = self
                    .overrides
                    .iter()
                    .find(|e| e.recurrence_id == event.range.start());

                if let Some(event_override) = event_override {
                    PrimitiveEvent {
                        range: event_override.range.clone(),
                        summary: event_override.summary.clone(),
                    }
                } else {
                    event
                }
            })
            .collect()
    }

    pub fn new(uid: String, events: Vec<CalendarEvent>) -> Result<Self> {
        anyhow::ensure!(!events.is_empty(), "Must specify at least one event");
        let mut overrides = Vec::with_capacity(events.len() - 1);
        let mut range = None;
        let mut summary = None;
        let mut recurrence = None;
        for event in events {
            if let Some(recurrence_id) = event.recurrence_id {
                overrides.push(EventOverride {
                    range: event.range,
                    summary: event.summary,
                    recurrence_id,
                })
            } else {
                anyhow::ensure!(
                    range.is_none(),
                    "Event without recurrence id must be only one"
                );
                range = Some(event.range);
                summary = Some(event.summary);
                recurrence = event.recurrence;
            }
        }
        if !overrides.is_empty() {
            anyhow::ensure!(
                recurrence.is_some(),
                "Recurrence id specified for event without recurrence rule"
            )
        }
        if let (Some(range), Some(summary)) = (range, summary) {
            Ok(EventSet {
                uid,
                range,
                summary,
                recurrence,
                overrides,
            })
        } else {
            anyhow::bail!("Must specify one event without recurrence id")
        }
    }
}
