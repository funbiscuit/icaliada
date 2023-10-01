use anyhow::Context;
use askama::Template;
use askama_axum::IntoResponse;
use axum::extract::Query;
use axum::Extension;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::config::AppConfig;
use crate::routes::error_response::{ApiError, ApiResult};
use crate::service::feeds::FeedService;

#[derive(Template)]
#[template(path = "feed.html")]
pub struct FeedTemplate {
    pub title: String,
    pub tokens: Vec<String>,
    pub colors: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct QueryParams {
    token: Option<String>,
    tokens: Option<String>,
}

pub async fn get_html_feed(
    Query(params): Query<QueryParams>,
    Extension(config): Extension<AppConfig>,
) -> ApiResult<impl IntoResponse> {
    if params.token.is_none() && params.tokens.is_none() {
        return Err(ApiError::NotFound("Token not present".to_string()));
    }

    let tokens = params
        .tokens
        .map(|tokens| tokens.split(',').map(|s| s.to_string()).collect::<Vec<_>>())
        .or(params.token.map(|token| vec![token]))
        .ok_or(ApiError::NotFound("Token not present".to_string()))?;

    let feeds = tokens
        .iter()
        .map(|t| config.get_feed_by_token(t))
        .collect::<Option<Vec<_>>>()
        .ok_or(ApiError::NotFound("Invalid token".to_string()))?;

    //TODO: move to config
    let colors = vec![
        "#E3826F".to_string(),
        "#E4A9A4".to_string(),
        "#EFBA97".to_string(),
        "#F1CCBB".to_string(),
        "#E7D5C7".to_string(),
    ];
    let title = feeds
        .iter()
        .map(|f| f.name.to_string())
        .collect::<Vec<_>>()
        .join(", ");

    Ok(FeedTemplate {
        title,
        tokens,
        colors,
    })
}

#[derive(Debug, Deserialize)]
pub struct EventsQuery {
    token: String,
    start: String,
    end: String,
}

pub async fn get_events_feed(
    Query(params): Query<EventsQuery>,
    Extension(feed): Extension<FeedService>,
) -> ApiResult<impl IntoResponse> {
    let start: DateTime<Utc> = params.start.parse().context("Invalid start datetime")?;
    let end: DateTime<Utc> = params.end.parse().context("Invalid end datetime")?;

    let events = feed.get_feed(&params.token, start, end).await?;

    #[derive(Clone, Debug, Serialize)]
    struct EventDto {
        start: String,
        end: String,
        title: String,
    }

    let fmt = "%Y-%m-%dT%H:%M:%SZ";
    let fmt_date = "%Y-%m-%d";

    let events: Vec<_> = events
        .into_iter()
        .map(|event| {
            let (start, end) = event.range.either(
                |start, end| {
                    (
                        start.format(fmt_date).to_string(),
                        end.format(fmt_date).to_string(),
                    )
                },
                |start, end| (start.format(fmt).to_string(), end.format(fmt).to_string()),
            );
            EventDto {
                start,
                end,
                title: event.summary,
            }
        })
        .collect();

    Ok(axum::Json(events))
}
