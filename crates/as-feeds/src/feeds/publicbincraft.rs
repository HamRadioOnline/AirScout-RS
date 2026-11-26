//! PublicBinCraft feed (`AirScout.PlaneFeeds.Plugin.PublicBinCraft`): the binCraft
//! endpoint of a public tar1090 globe site, limited to the covered area.
//!
//! Unlike the C# plugin this feed does not imitate a Chrome browser (no browser
//! User-Agent, `Sec-CH-UA`, `Sec-Fetch-*` or Referer headers): it identifies itself with
//! the AirScout-RS User-Agent. It keeps a cookie store, as C# does.

use super::bincraft_parse::parse_public;
use super::{client, get_bytes, set_bool, set_str, unzstd};
use crate::vars::format_double;
use crate::{FeedArgs, FeedError, FeedPlane, PlaneFeed};
use reqwest::blocking::Client;

/// `PublicBinCraft.cs:27` at fbadef0: no public host is preset any more.
pub const DEFAULT_HOST: &str = "http://localhost";

/// `PublicBinCraft.cs` `Disclaimer` at fbadef0: the six C# string pieces joined with `+`,
/// verbatim (typo "auhor" included).
const DISCLAIMER: &str = concat!(
    "This plane feed might fetch data from an Internet server via the binCraft endpoint.\n",
    "The use is probably not intended by the website owners and could be changed in URL and data format frequently and without further notice.\n",
    "Furthermore, it might cause legal issues in some countries.\n",
    "By clicking on \"Accept\" you understand that you are\n\n",
    "DOING THAT ON YOUR OWN RISK\n\n",
    "The auhor of this software will not be responsible in any case.",
);

/// `HttpWebRequest` default timeout.
const TIMEOUT_S: u64 = 100;

/// The PublicBinCraft feed.
#[derive(Debug, Clone)]
pub struct PublicBinCraft {
    pub host: String,
    pub compressed: bool,
    client: Option<Client>,
}

impl Default for PublicBinCraft {
    fn default() -> Self {
        Self {
            host: DEFAULT_HOST.into(),
            compressed: true,
            client: None,
        }
    }
}

impl PublicBinCraft {
    /// `<scheme>://<host>[:port]/re-api/?binCraft&zstd&box=minLat,maxLat,minLon,maxLon`;
    /// any path in the host setting is dropped (`GetLeftPart(UriPartial.Authority)`).
    pub fn url(&self, args: &FeedArgs) -> Result<String, FeedError> {
        let u =
            reqwest::Url::parse(&self.host).map_err(|e| FeedError::Config(format!("host: {e}")))?;
        let base = u.origin().ascii_serialization();
        Ok(format!(
            "{base}/re-api/?binCraft&zstd&box={},{},{},{}",
            format_double(args.min_lat),
            format_double(args.max_lat),
            format_double(args.min_lon),
            format_double(args.max_lon)
        ))
    }
}

impl PlaneFeed for PublicBinCraft {
    fn name(&self) -> &'static str {
        "PublicBinCraft"
    }

    fn info(&self) -> &'static str {
        "Web feed from the binCraft endpoint of a public tar1090 globe site \
         (for example globe.airplanes.live; the default host is http://localhost), limited to the covered area."
    }

    fn disclaimer(&self) -> Option<&'static str> {
        Some(DISCLAIMER)
    }

    fn enabled_by_default(&self) -> bool {
        false
    }

    fn settings(&self) -> toml::Table {
        let mut t = toml::Table::new();
        t.insert("host".into(), self.host.clone().into());
        t.insert("compressed".into(), self.compressed.into());
        t
    }

    fn load_settings(&mut self, t: &toml::Table) -> Result<(), FeedError> {
        set_str(t, "host", &mut self.host)?;
        set_bool(t, "compressed", &mut self.compressed)
    }

    fn start(&mut self, _args: &FeedArgs) -> Result<(), FeedError> {
        self.client = Some(client(TIMEOUT_S, true)?);
        Ok(())
    }

    fn get_planes(&mut self, args: &FeedArgs) -> Result<Vec<FeedPlane>, FeedError> {
        let url = self.url(args)?;
        if self.client.is_none() {
            self.client = Some(client(TIMEOUT_S, true)?);
        }
        let Some(c) = self.client.as_ref() else {
            return Err(FeedError::Http("no HTTP client".into()));
        };
        let body = get_bytes(c, &url, None)?;
        let data = if self.compressed {
            unzstd(&body)?
        } else {
            body
        };
        parse_public(&data)
    }

    fn stop(&mut self, _args: &FeedArgs) -> Result<(), FeedError> {
        self.client = None;
        Ok(())
    }
}
