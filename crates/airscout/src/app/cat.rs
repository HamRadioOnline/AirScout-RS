//! Events of the CAT worker in the app (`MapDlg.cs:9252-9331`): status text, CAT field, rig
//! identification and the dial frequency the rig reports.

use super::*;
use crate::tracking::TrackStatus;
use as_cat::{CatEvent, MAX_FREQ_HZ};

impl AppState {
    /// Tooltip of the CAT field (`SayCAT`, `:9315-9325`).
    pub fn cat_tooltip(&self) -> String {
        match &self.cat_ident {
            Some(i) => format!(
                "{}\n{}\n\n{}",
                i.cat_version,
                i.rig_type,
                self.cat_status.name()
            ),
            None => format!("CAT error!\n\n{}", self.cat_status.name()),
        }
    }

    /// An event of the CAT worker of `generation`; one of an older worker is dropped.
    pub(super) fn on_cat_event(&mut self, generation: u64, e: CatEvent) {
        if generation != self.integration_gen {
            return;
        }
        match e {
            CatEvent::Text(t) => self.status = t,
            CatEvent::Status(s) => self.cat_status = s,
            CatEvent::Ident(i) => self.cat_ident = i,
            CatEvent::Info(i) => {
                self.status = crate::cat_service::rig_reports(&i);
                // the rig's dial feeds the Doppler base unless tracking drives it (:9293-9301;
                // SINGLE and STOPPED do update). Never 0 Hz: that would zero the base. Never
                // above the Options limit: the Doppler maths must not overflow. Never a value read
                // while the Doppler drove the rig: the Track worker can drive it before this
                // thread shows TRACKING (a minimised window), and a shifted base would stack.
                if self.track_status != TrackStatus::Tracking
                    && !i.doppler_active
                    && i.rx_hz > 0
                    && i.rx_hz <= MAX_FREQ_HZ
                {
                    self.settings.doppler.dial_freq_hz = i.rx_hz;
                }
            }
        }
    }
}
