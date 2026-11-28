//! Scheduling and fetching the Latest News page's modification time.

use chrono::{DateTime, Utc};
use reqwest::blocking::Client;
use reqwest::header::LAST_MODIFIED;
use std::time::{Duration, Instant};

pub fn news_due(last_check: Option<Instant>, now: Instant, interval_s: u64, enabled: bool) -> bool {
    enabled
        && last_check.is_none_or(|last| {
            now.saturating_duration_since(last) >= Duration::from_secs(interval_s)
        })
}

/// Like `GetWebCreationTimeUtc`, follow redirects and inspect the GET response headers.
/// Dropping the response without reading its body keeps the check independent of page size.
pub(crate) fn last_modified(url: &str) -> Result<DateTime<Utc>, String> {
    let response = Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent(as_net::user_agent())
        .build()
        .and_then(|client| client.get(url).send())
        .and_then(|response| response.error_for_status())
        .map_err(|error| error.without_url().to_string())?;
    let value = response
        .headers()
        .get(LAST_MODIFIED)
        .ok_or("missing Last-Modified header")?;
    value
        .to_str()
        .ok()
        .and_then(|value| as_net::parse_http_date(value).ok())
        .map(DateTime::from)
        .ok_or_else(|| "invalid Last-Modified header".into())
}
