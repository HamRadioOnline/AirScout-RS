//! BinCraft feed (`AirScout.PlaneFeeds.Plugin.BinCraft`): a tar1090 `aircraft.binCraft.zst`
//! file from one receiver, decoded by [`parse_bincraft`].

use super::bincraft_parse::parse_bincraft;
use super::{client, get_bytes, set_bool, set_str, unzstd, WEBCLIENT_TIMEOUT_S};
use crate::{vars, FeedArgs, FeedError, FeedPlane, PlaneFeed};
use chrono::Utc;

pub const DEFAULT_URL: &str = "http://44.149.131.185/tar1090/data/aircraft.binCraft.zst";

/// The BinCraft feed.
#[derive(Debug, Clone)]
pub struct BinCraft {
    pub url: String,
    pub compressed: bool,
}

impl Default for BinCraft {
    fn default() -> Self {
        Self {
            url: DEFAULT_URL.into(),
            compressed: true,
        }
    }
}

impl PlaneFeed for BinCraft {
    fn name(&self) -> &'static str {
        "BinCraft"
    }

    fn info(&self) -> &'static str {
        "Web feed from a tar1090 receiver in binCraft format (aircraft.binCraft.zst)."
    }

    fn disclaimer(&self) -> Option<&'static str> {
        None
    }

    fn enabled_by_default(&self) -> bool {
        false
    }

    fn settings(&self) -> toml::Table {
        let mut t = toml::Table::new();
        t.insert("url".into(), self.url.clone().into());
        t.insert("compressed".into(), self.compressed.into());
        t
    }

    fn load_settings(&mut self, t: &toml::Table) -> Result<(), FeedError> {
        set_str(t, "url", &mut self.url)?;
        set_bool(t, "compressed", &mut self.compressed)
    }

    fn start(&mut self, _args: &FeedArgs) -> Result<(), FeedError> {
        Ok(())
    }

    fn get_planes(&mut self, args: &FeedArgs) -> Result<Vec<FeedPlane>, FeedError> {
        let url = vars::substitute(&self.url, args, Utc::now(), "")?;
        let body = get_bytes(&client(WEBCLIENT_TIMEOUT_S, false)?, &url, None)?;
        let data = if self.compressed {
            unzstd(&body)?
        } else {
            body
        };
        parse_bincraft(&data)
    }

    fn stop(&mut self, _args: &FeedArgs) -> Result<(), FeedError> {
        Ok(())
    }
}
