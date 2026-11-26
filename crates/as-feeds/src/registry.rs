//! All feeds known to the application.

use crate::feeds::{
    adsbone, adsbsupport, airscoutserver, bincraft, dump1090, flexjson, opensky, publicbincraft,
    readsb, rtl1090, vrs,
};
use crate::PlaneFeed;

/// One fresh instance of every feed, in UI order.
pub fn all_feeds() -> Vec<Box<dyn PlaneFeed>> {
    vec![
        Box::new(opensky::OpenSky::default()),
        Box::new(adsbone::AdsbOne::default()),
        Box::new(readsb::Readsb::default()),
        Box::new(dump1090::Dump1090::default()),
        Box::new(airscoutserver::AirScoutServer::default()),
        Box::new(vrs::Vrs::default()),
        Box::new(bincraft::BinCraft::default()),
        Box::new(publicbincraft::PublicBinCraft::default()),
        Box::new(adsbsupport::AdsbSupport::default()),
        Box::new(rtl1090::Rtl1090::default()),
        Box::new(flexjson::FlexJson::default()),
    ]
}

/// A fresh instance of the feed called `name`.
pub fn feed_by_name(name: &str) -> Option<Box<dyn PlaneFeed>> {
    all_feeds().into_iter().find(|f| f.name() == name)
}
