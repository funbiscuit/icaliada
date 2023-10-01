use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use chrono::{DateTime, Utc};
use futures::future;
use ical::parser::ical::component::IcalCalendar;
use moka::future::Cache;
use reqwest::Client;
use secrecy::ExposeSecret;

use crate::config::CalendarConfig;
use crate::model::{CalendarEvent, EventSet, PrimitiveEvent, Timezone};
use crate::service::config::AppConfig;

#[derive(Clone)]
pub struct FeedService {
    config: AppConfig,
    cache: Arc<Cache<CalendarConfig, Vec<u8>>>,
}

impl FeedService {
    pub fn new(config: &AppConfig) -> Self {
        Self {
            config: config.clone(),
            cache: Arc::new(
                Cache::builder()
                    .time_to_live(Duration::from_secs(60))
                    .build(),
            ),
        }
    }

    pub async fn get_feed(
        &self,
        token: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> anyhow::Result<Vec<PrimitiveEvent>> {
        let config = self
            .config
            .get_feed_by_token(token)
            .context("Invalid token")?;
        let is_public = token == config.tokens.public.expose_secret();

        let events_futures: Vec<_> = config
            .calendars
            .iter()
            .map(|calendar| self.fetch_calendar_events(calendar, start, end))
            .collect();

        let events = future::join_all(events_futures)
            .await
            .into_iter()
            .filter_map(|res| {
                if let Err(err) = res {
                    tracing::error!("Failed to fetch calendar: {:?}", err);
                    None
                } else {
                    res.ok()
                }
            })
            .flatten()
            .map(|event| {
                if is_public {
                    PrimitiveEvent {
                        range: event.range,
                        summary: "Busy".to_string(),
                    }
                } else {
                    event
                }
            })
            .collect();

        Ok(events)
    }

    async fn fetch_calendar_events(
        &self,
        calendar: &CalendarConfig,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> anyhow::Result<Vec<PrimitiveEvent>> {
        let client = Client::builder().build().unwrap();

        let cached = self.cache.get(calendar).await;

        let bytes = if let Some(cached) = cached {
            tracing::info!("Using cached events: {:?}", cached.len());
            cached
        } else {
            let response = client
                .get(calendar.url.expose_secret())
                .send()
                .await
                .context("Failed to get calendar from url")?;
            let bytes = response
                .bytes()
                .await
                .context("Failed to get calendar bytes")?;
            let bytes = bytes.to_vec();
            tracing::info!("Downloaded ical: {}", bytes.len());
            self.cache.insert(calendar.clone(), bytes.clone()).await;
            bytes
        };

        let reader = ical::IcalParser::new(bytes.as_ref());

        let mut calendar_events = vec![];
        for calendar in reader.flatten() {
            let mut new_events = create_events(calendar, start, end);
            calendar_events.append(&mut new_events);
        }
        Ok(calendar_events)
    }
}

fn create_events(
    calendar: IcalCalendar,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Vec<PrimitiveEvent> {
    let timezones: HashMap<_, _> = calendar
        .timezones
        .into_iter()
        .filter_map(|cal_tz| Timezone::try_from(cal_tz).ok())
        .map(|tz| (tz.id().to_string(), tz))
        .collect();

    let mut events: HashMap<_, Vec<CalendarEvent>> = HashMap::new();

    calendar
        .events
        .into_iter()
        .filter_map::<CalendarEvent, _>(|e| {
            CalendarEvent::from_ical_event(e, |tz, time| {
                timezones.get(tz).unwrap().local_to_utc(time)
            })
            .map_err(|e| tracing::error!("Failed to convert: {:?}", e))
            .ok()
        })
        .for_each(|event| {
            events.entry(event.uid.clone()).or_default().push(event);
        });

    events
        .into_iter()
        .filter_map(|(id, events)| {
            EventSet::new(id, events)
                .map_err(|e| tracing::error!("Failed to create event set: {:?}", e))
                .ok()
        })
        .flat_map(|set| set.create_primitives(start, end))
        .collect()
}
