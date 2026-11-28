//! App wiring of CAT (stage 4b): rig events, dial frequency, CAT field, restore on exit.
use airscout::app::{AppState, Services};
use airscout::cat_service::doppler_of;
use airscout::services::{PathRequest, ServiceEvent};
use airscout::tracker::TrackInput;
use airscout::tracking::TrackStatus;
use airscout::ui::status;
use as_aircraft::PlaneInfoCache;
use as_cat::{CatEvent, RigIdent, RigInfo, RigMode, RigRit, RigSplit, RigStatus, MAX_FREQ_HZ};
use as_settings::{DopplerStrategy, Settings};
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

#[derive(Default)]
struct Log {
    starts: Vec<u64>,
    stops: usize,
    waits: Vec<Duration>,
    /// Every `track_input` push.
    inputs: Vec<TrackInput>,
    /// The input cell's contents at each `start_integrations`: what the new Track worker's first
    /// step reads.
    input_at_start: Vec<Option<TrackInput>>,
}

#[derive(Clone, Default)]
struct Rec(Rc<RefCell<Log>>);

impl Services for Rec {
    fn request_path(&mut self, _r: PathRequest) {}
    fn start_feed(&mut self, _: u64, _: &Settings, _: Arc<PlaneInfoCache>) -> Result<(), String> {
        Ok(())
    }
    fn stop_feed(&mut self) {}
    fn start_integrations(&mut self, _s: &Settings, generation: u64, _c: Arc<PlaneInfoCache>) {
        let mut l = self.0.borrow_mut();
        l.starts.push(generation);
        let last = l.inputs.last().cloned();
        l.input_at_start.push(last);
    }
    fn stop_integrations(&mut self) {
        self.0.borrow_mut().stops += 1;
    }
    fn stop_integrations_and_wait(&mut self, max: Duration) {
        self.0.borrow_mut().waits.push(max);
    }
    fn track_input(&mut self, i: TrackInput) {
        self.0.borrow_mut().inputs.push(i);
    }
}

fn state(rec: &Rec) -> AppState {
    let s = Settings {
        first_run: false,
        ..Settings::default()
    };
    AppState::new(
        s,
        None,
        Box::new(rec.clone()),
        Arc::new(PlaneInfoCache::new()),
    )
}

fn info(rx_hz: i64) -> RigInfo {
    RigInfo {
        rx_hz,
        tx_hz: rx_hz,
        mode: RigMode::Usb,
        split: RigSplit::SplitOff,
        rit: RigRit::RitOff,
        doppler_active: false,
    }
}

fn cat(generation: u64, event: CatEvent) -> ServiceEvent {
    ServiceEvent::Cat { generation, event }
}

#[test]
fn rig_info_sets_dial_and_status_text() {
    let mut st = state(&Rec::default());
    st.start_integrations();
    st.on_event(cat(st.integration_gen, CatEvent::Info(info(144_300_000))));
    assert_eq!(st.settings.doppler.dial_freq_hz, 144_300_000);
    assert_eq!(
        st.status,
        "Rig reports RX: 144300000Hz, TX: 144300000Hz, Mode: USB, RIT: OFF, Split: OFF"
    );
}

#[test]
fn rig_info_not_applied_while_tracking() {
    let mut st = state(&Rec::default());
    st.start_integrations();
    st.settings.doppler.dial_freq_hz = 144_300_000;
    st.track_status = TrackStatus::Tracking;
    st.on_event(cat(st.integration_gen, CatEvent::Info(info(144_300_312))));
    assert_eq!(st.settings.doppler.dial_freq_hz, 144_300_000);
    assert!(st.status.contains("RX: 144300312Hz"));
}

/// The worker may drive the rig before the UI shows TRACKING (a minimised window): an Info of a
/// Doppler pass never becomes the dial base.
#[test]
fn rig_info_of_doppler_pass_not_applied() {
    for ts in [TrackStatus::Single, TrackStatus::Stopped, TrackStatus::None] {
        let mut st = state(&Rec::default());
        st.start_integrations();
        st.settings.doppler.dial_freq_hz = 144_300_000;
        st.track_status = ts;
        let i = RigInfo {
            doppler_active: true,
            ..info(144_300_312)
        };
        st.on_event(cat(st.integration_gen, CatEvent::Info(i)));
        assert_eq!(st.settings.doppler.dial_freq_hz, 144_300_000, "{ts:?}");
        assert!(st.status.contains("RX: 144300312Hz"));
    }
}

#[test]
fn rig_info_applied_while_single_or_stopped() {
    for ts in [TrackStatus::Single, TrackStatus::Stopped, TrackStatus::None] {
        let mut st = state(&Rec::default());
        st.start_integrations();
        st.track_status = ts;
        st.on_event(cat(st.integration_gen, CatEvent::Info(info(144_300_312))));
        assert_eq!(st.settings.doppler.dial_freq_hz, 144_300_312, "{ts:?}");
    }
}

#[test]
fn rig_info_zero_leaves_dial_unchanged() {
    let mut st = state(&Rec::default());
    st.start_integrations();
    st.settings.doppler.dial_freq_hz = 144_300_000;
    st.on_event(cat(st.integration_gen, CatEvent::Info(info(0))));
    assert_eq!(st.settings.doppler.dial_freq_hz, 144_300_000);
    assert!(st.status.contains("RX: 0Hz"));
}

#[test]
fn rig_info_over_limit_leaves_dial_unchanged() {
    let mut st = state(&Rec::default());
    st.start_integrations();
    st.settings.doppler.dial_freq_hz = 144_300_000;
    for hz in [MAX_FREQ_HZ + 1, i64::MAX] {
        st.on_event(cat(st.integration_gen, CatEvent::Info(info(hz))));
        assert_eq!(st.settings.doppler.dial_freq_hz, 144_300_000, "{hz}");
    }
    st.on_event(cat(st.integration_gen, CatEvent::Info(info(MAX_FREQ_HZ))));
    assert_eq!(st.settings.doppler.dial_freq_hz, MAX_FREQ_HZ);
}

#[test]
fn stale_cat_events_dropped() {
    let mut st = state(&Rec::default());
    st.start_integrations();
    let old = st.integration_gen - 1;
    st.on_event(cat(old, CatEvent::Status(RigStatus::Online)));
    st.on_event(cat(old, CatEvent::Text("late".into())));
    st.on_event(cat(old, CatEvent::Info(info(144_300_000))));
    assert_eq!(st.cat_status, RigStatus::None);
    assert_ne!(st.status, "late");
    assert_eq!(
        st.settings.doppler.dial_freq_hz,
        Settings::default().doppler.dial_freq_hz
    );
}

#[test]
fn text_status_and_ident_events() {
    let mut st = state(&Rec::default());
    st.start_integrations();
    let g = st.integration_gen;
    st.on_event(cat(g, CatEvent::Text("hello".into())));
    assert_eq!(st.status, "hello");
    st.on_event(cat(g, CatEvent::Status(RigStatus::Offline)));
    assert_eq!(st.cat_status, RigStatus::Offline);
    let id = RigIdent {
        cat_version: "v".into(),
        rig_type: "r".into(),
    };
    st.on_event(cat(g, CatEvent::Ident(Some(id.clone()))));
    assert_eq!(st.cat_ident, Some(id));
    st.on_event(cat(g, CatEvent::Ident(None)));
    assert_eq!(st.cat_ident, None);
}

#[test]
fn cat_tooltip_texts() {
    let mut st = state(&Rec::default());
    st.cat_status = RigStatus::Online;
    st.cat_ident = Some(RigIdent {
        cat_version: "Hamlib rigctld (net protocol)".into(),
        rig_type: "[rigctld] localhost:4532".into(),
    });
    assert_eq!(
        st.cat_tooltip(),
        "Hamlib rigctld (net protocol)\n[rigctld] localhost:4532\n\nONLINE"
    );
    st.cat_ident = None;
    st.cat_status = RigStatus::NoRig;
    assert_eq!(st.cat_tooltip(), "CAT error!\n\nNORIG");
}

#[test]
fn stop_integrations_resets_cat_status() {
    let rec = Rec::default();
    let mut st = state(&rec);
    st.start_integrations();
    st.on_event(cat(st.integration_gen, CatEvent::Status(RigStatus::Online)));
    assert_eq!(st.cat_status, RigStatus::Online);
    st.stop_integrations();
    assert_eq!(st.cat_status, RigStatus::None);
    assert_eq!(rec.0.borrow().stops, 1);
}

#[test]
fn start_integrations_resets_cat_status() {
    let mut st = state(&Rec::default());
    st.cat_status = RigStatus::Online;
    st.start_integrations();
    assert_eq!(st.cat_status, RigStatus::None);
}

#[test]
fn exit_waits_for_cat() {
    let rec = Rec::default();
    let mut st = state(&rec);
    st.start_integrations();
    let g = st.integration_gen;
    st.on_exit();
    assert_eq!(rec.0.borrow().waits, vec![Duration::from_secs(2)]);
    assert_eq!(rec.0.borrow().stops, 0);
    assert_eq!(st.integration_gen, g + 1);
    assert_eq!(st.cat_status, RigStatus::None);
}

#[test]
fn history_on_stops_cat() {
    let rec = Rec::default();
    let mut st = state(&rec);
    st.start_integrations();
    st.on_event(cat(st.integration_gen, CatEvent::Status(RigStatus::Online)));
    st.enter_history().unwrap();
    assert_eq!(rec.0.borrow().stops, 1);
    assert_eq!(st.cat_status, RigStatus::None);
}

#[test]
fn status_bar_shows_cat_after_rot() {
    let mut st = state(&Rec::default());
    st.cat_status = RigStatus::Online;
    let h = Harness::new_ui_state(
        |ui, st: &mut AppState| {
            let rect = ui.ctx().viewport_rect();
            status::show(ui, st, rect);
        },
        st,
    );
    let rot = h.get_by_label("ROT").rect();
    let cat = h.get_by_label("CAT").rect();
    assert!(cat.min.x > rot.max.x, "{cat:?} {rot:?}");
}

/// Options OK restarts the Track worker, which steps at once from the input cell. The cell holds
/// the input of the closed dialog by then: a Doppler strategy switched to None is not pushed to
/// the CAT worker once with the old strategy.
#[test]
fn options_ok_pushes_new_input_before_worker_start() {
    let rec = Rec::default();
    let mut st = state(&rec);
    st.settings.track.activate = true;
    st.settings.doppler.strategy = DopplerStrategy::A;
    st.startup();
    st.track_sync();
    assert!(st.open_options_dlg());
    // the dialog's OK writes the new settings, then closes
    st.settings.doppler.strategy = DopplerStrategy::None;
    st.options_dlg = None;
    st.options_closed();
    let l = rec.0.borrow();
    let first = l.input_at_start.last().unwrap().as_ref().expect("input");
    let p = first.params.as_ref().expect("params");
    assert_eq!(p.strategy, DopplerStrategy::None);
    assert!(!doppler_of(p.strategy).is_active());
}
