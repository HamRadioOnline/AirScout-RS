//! App wiring of the integrations (stage 4a): Win-Test receiver and Track worker start, stop and
//! restart around the Options dialog (`MapDlg.cs:3893`, `:3976`), the track steps (also while
//! paused, as `bw_Track` runs whenever `Track_Activate` is on), the ROT field and the live
//! nearest settings.
use airscout::app::{AppState, RealServices, Services};
use airscout::net_addrs::NetAddrs;
use airscout::services::{open_data, PathRequest, ServiceEvent};
use airscout::track_outputs::{OutputGroup, NEWLINE};
use airscout::track_service::TrackEvent;
use airscout::tracker::{TrackInput, TrackParams, TrackShown, Tracker};
use airscout::tracking::{RotStatus, TrackStatus, TrackStep, TrackValues};
use airscout::ui::{control, dialogs};
use airscout::wintest_nearest::NearestSettings;
use as_aircraft::enums::TrackMode;
use as_aircraft::PlaneInfoCache;
use as_geo::band::Band;
use as_geo::latlon::{bearing, GPoint};
use as_settings::{Settings, TrackFile, TrackUdp};
use chrono::{DateTime, TimeZone, Utc};
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use std::cell::RefCell;
use std::net::{SocketAddr, UdpSocket};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Default)]
struct Log {
    /// Generation of every `start_integrations`.
    starts: Vec<u64>,
    stops: usize,
    /// Every `track_input` push.
    inputs: Vec<TrackInput>,
    /// The steps of the fake worker (`Rec::worker_step`).
    steps: Vec<TrackStep>,
    tracker: Tracker,
    /// What `track_shown` answers.
    shown: Option<TrackShown>,
    height_requests: usize,
    nearest: Vec<NearestSettings>,
    /// `my_call` of every `set_web_settings` push.
    web: Vec<String>,
}

#[derive(Clone, Default)]
struct Rec(Rc<RefCell<Log>>);

impl Services for Rec {
    fn request_path(&mut self, _r: PathRequest) {}
    fn start_feed(&mut self, _: u64, _: &Settings, _: Arc<PlaneInfoCache>) -> Result<(), String> {
        Ok(())
    }
    fn stop_feed(&mut self) {}
    fn request_ground_elevations(&mut self, _id: u64, _my: GPoint, _dx: GPoint) {
        self.0.borrow_mut().height_requests += 1;
    }
    fn start_integrations(&mut self, _s: &Settings, generation: u64, _c: Arc<PlaneInfoCache>) {
        self.0.borrow_mut().starts.push(generation);
    }
    fn stop_integrations(&mut self) {
        self.0.borrow_mut().stops += 1;
    }
    fn track_input(&mut self, i: TrackInput) {
        self.0.borrow_mut().inputs.push(i);
    }
    fn track_shown(&self) -> Option<TrackShown> {
        self.0.borrow().shown.clone()
    }
    fn set_nearest_settings(&mut self, n: NearestSettings) {
        self.0.borrow_mut().nearest.push(n);
    }
    fn set_web_settings(&mut self, s: &Settings) {
        self.0.borrow_mut().web.push(s.my_call.clone());
    }
}

impl Rec {
    /// One step of a fake Track worker over the last pushed input (empty plane cache), as the
    /// real worker does once a second.
    fn worker_step(&self, now: DateTime<Utc>) {
        let mut l = self.0.borrow_mut();
        let Some(input) = l.inputs.last().cloned() else {
            return;
        };
        if let Some((step, shown)) = l.tracker.step(&input, now, &PlaneInfoCache::new()) {
            l.steps.push(step);
            l.shown = Some(shown);
        }
    }
}

/// One frame, one worker step, one frame: what the UI sees of a step.
fn sync_step(st: &mut AppState, rec: &Rec) {
    st.track_sync();
    rec.worker_step(t0());
    st.track_sync();
}

fn t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 24, 12, 0, 0).unwrap()
}

/// My JO50AA-ish, DX JO60AA-ish, no first run, tracking on.
fn settings() -> Settings {
    let mut s = Settings {
        my_lat: 50.02,
        my_lon: 10.04,
        dx_lat: 50.02,
        dx_lon: 12.04,
        first_run: false,
        ..Settings::default()
    };
    s.my_call = "TE1ST".into();
    s.dx_call = "TE2ND".into();
    s.track.activate = true;
    s
}

fn state(s: Settings, rec: &Rec) -> AppState {
    AppState::new(
        s,
        None,
        Box::new(rec.clone()),
        Arc::new(PlaneInfoCache::new()),
    )
}

#[test]
fn startup_starts_integrations() {
    let rec = Rec::default();
    let mut st = state(settings(), &rec);
    st.startup();
    assert_eq!(rec.0.borrow().starts, vec![1]);
    assert_eq!(st.integration_gen, 1);
}

#[test]
fn wizard_finish_starts_integrations() {
    let rec = Rec::default();
    let mut st = state(Settings::default(), &rec);
    st.startup();
    assert!(st.wizard.is_some());
    assert!(rec.0.borrow().starts.is_empty());
    st.finish_wizard();
    assert_eq!(rec.0.borrow().starts, vec![1]);
    assert_eq!(st.integration_gen, 1);
}

fn harness(st: AppState) -> Harness<'static, AppState> {
    Harness::new_ui_state(
        |ui, st: &mut AppState| {
            control::show(ui, st);
            dialogs::show(ui.ctx(), st);
        },
        st,
    )
}

fn open(h: &mut Harness<'static, AppState>) {
    h.get_by_label("Options").click();
    h.run();
    assert!(h.state().options_dlg.is_some());
}

#[test]
fn options_open_stops_and_close_restarts() {
    let rec = Rec::default();
    let mut st = state(settings(), &rec);
    st.startup();
    assert!(st.open_options_dlg());
    assert_eq!(rec.0.borrow().stops, 1);
    assert_eq!(rec.0.borrow().starts, vec![1]);
    st.options_dlg = None;
    st.options_closed();
    assert_eq!(rec.0.borrow().starts, vec![1, 3]);
    assert_eq!(st.integration_gen, 3);

    // the dialog's own buttons: Cancel and OK both restart
    let mut h = harness(st);
    open(&mut h);
    assert_eq!(rec.0.borrow().stops, 2);
    h.get_by_label("Cancel").click();
    h.run();
    assert!(h.state().options_dlg.is_none());
    assert_eq!(rec.0.borrow().starts, vec![1, 3, 5]);
    open(&mut h);
    assert_eq!(rec.0.borrow().stops, 3);
    h.get_by_label("OK").click();
    h.run();
    assert!(h.state().options_dlg.is_none());
    assert_eq!(rec.0.borrow().starts, vec![1, 3, 5, 7]);
    assert_eq!(h.state().integration_gen, 7);
}

#[test]
fn options_close_in_history_restarts_integrations() {
    let rec = Rec::default();
    let mut st = state(settings(), &rec);
    st.startup();
    st.rot_status = RotStatus::Tracking;
    st.enter_history().unwrap();
    // ON stops the integrations (`:6558`); ROT reads NONE in HISTORY
    assert_eq!(rec.0.borrow().stops, 1);
    assert_eq!(st.rot_status, RotStatus::None);
    assert!(st.open_options_dlg());
    assert_eq!(rec.0.borrow().stops, 2);
    st.options_dlg = None;
    st.options_closed();
    // closing Options in HISTORY restarts them (`:3979`)
    assert_eq!(rec.0.borrow().starts, vec![1, 4]);
    // OFF starts them again (`:6585`)
    st.leave_history();
    assert_eq!(rec.0.borrow().starts, vec![1, 4, 5]);
    assert_eq!(rec.0.borrow().stops, 2);
}

fn track(generation: u64, event: TrackEvent) -> ServiceEvent {
    ServiceEvent::Track { generation, event }
}

#[test]
fn stale_track_events_dropped() {
    let rec = Rec::default();
    let mut st = state(settings(), &rec);
    st.start_integrations();
    st.start_integrations();
    assert_eq!(st.integration_gen, 2);
    st.on_event(track(1, TrackEvent::Rot(RotStatus::Tracking)));
    assert_eq!(st.rot_status, RotStatus::None);
    st.on_event(track(1, TrackEvent::Text("old".into())));
    assert_ne!(st.status, "old");
    st.on_event(track(2, TrackEvent::Rot(RotStatus::Tracking)));
    assert_eq!(st.rot_status, RotStatus::Tracking);
    // stop reports NONE (`:8342-8343`); a late event of the stopped worker cannot undo it
    st.stop_integrations();
    assert_eq!(st.rot_status, RotStatus::None);
    st.on_event(track(2, TrackEvent::Rot(RotStatus::Stopped)));
    assert_eq!(st.rot_status, RotStatus::None);
    // a restart begins with NONE too
    st.start_integrations();
    st.on_event(track(st.integration_gen, TrackEvent::Rot(RotStatus::Error)));
    assert_eq!(st.rot_status, RotStatus::Error);
    st.start_integrations();
    assert_eq!(st.rot_status, RotStatus::None);
}

#[test]
fn track_disabled_clears_udp_setting_and_sets_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.toml");
    let rec = Rec::default();
    let mut s = settings();
    s.track.udp = TrackUdp::WinTest;
    s.track.file = TrackFile::Native;
    let mut st = state(s, &rec);
    st.settings_path = Some(path.clone());
    st.start_integrations();
    let g = st.integration_gen;
    st.on_event(track(
        g,
        TrackEvent::Text("Tracking via UDP disabled.".into()),
    ));
    assert_eq!(st.status, "Tracking via UDP disabled.");
    st.on_event(track(g, TrackEvent::Disabled(OutputGroup::Udp)));
    assert_eq!(st.settings.track.udp, TrackUdp::None);
    assert_eq!(st.settings.track.file, TrackFile::Native);
    assert_eq!(st.track_status, TrackStatus::Error);
    st.on_event(track(g, TrackEvent::Disabled(OutputGroup::File)));
    assert_eq!(st.settings.track.file, TrackFile::None);
    // persisted (C# `Track_UDP_None = true`, :8224): a restart does not switch it on again
    st.on_exit();
    let saved = Settings::load_or_default(&path).unwrap();
    assert_eq!(saved.track.udp, TrackUdp::None);
    assert_eq!(saved.track.file, TrackFile::None);
}

#[test]
fn stale_disabled_event_changes_nothing() {
    let rec = Rec::default();
    let mut s = settings();
    s.track.udp = TrackUdp::AirScout;
    let mut st = state(s, &rec);
    st.start_integrations();
    st.start_integrations();
    st.on_event(track(1, TrackEvent::Disabled(OutputGroup::Udp)));
    assert_eq!(st.settings.track.udp, TrackUdp::AirScout);
    assert_ne!(st.track_status, TrackStatus::Error);
}

#[test]
fn single_while_paused_steps_outputs() {
    let rec = Rec::default();
    let mut st = state(settings(), &rec);
    assert!(!st.is_playing());
    st.right_click_dx();
    sync_step(&mut st, &rec);
    let want = bearing(50.02, 10.04, 50.02, 12.04);
    {
        let l = rec.0.borrow();
        assert_eq!(l.steps.len(), 1);
        let step = &l.steps[0];
        assert_eq!(step.mode, TrackMode::Single);
        let v = step.values.as_ref().expect("valid values");
        assert_eq!(v.my_azimuth, want);
        assert_eq!(v.my_elevation, 0.0);
        assert_eq!(l.height_requests, 0);
    }
    assert_eq!(st.track_status, TrackStatus::Stopped);
    // SINGLE stays set and is processed every second until Pause or Play resets it
    sync_step(&mut st, &rec);
    assert_eq!(rec.0.borrow().steps.len(), 2);
    assert_eq!(rec.0.borrow().steps[1].mode, TrackMode::Single);
    st.pause();
    sync_step(&mut st, &rec);
    let l = rec.0.borrow();
    assert_eq!(l.steps.len(), 3);
    assert_eq!(
        l.steps[2],
        TrackStep {
            values: None,
            mode: TrackMode::None
        }
    );
    assert_eq!(l.height_requests, 0);
}

#[test]
fn paused_none_mode_sends_invalid_step() {
    let rec = Rec::default();
    let mut st = state(settings(), &rec);
    sync_step(&mut st, &rec);
    let l = rec.0.borrow();
    assert_eq!(
        l.steps,
        vec![TrackStep {
            values: None,
            mode: TrackMode::None
        }]
    );
    assert_eq!(l.height_requests, 0);
    // values that are not valid report STOPPED (:8300-8301)
    assert_eq!(st.track_status, TrackStatus::Stopped);
}

#[test]
fn track_without_heights_sends_invalid_step() {
    let rec = Rec::default();
    let mut st = state(settings(), &rec);
    st.play().unwrap();
    st.track_mode = TrackMode::Track;
    st.selected = Some("TST123".into());
    sync_step(&mut st, &rec);
    let l = rec.0.borrow();
    assert_eq!(l.height_requests, 1);
    assert_eq!(
        l.steps,
        vec![TrackStep {
            values: None,
            mode: TrackMode::Track
        }]
    );
}

/// The worker owns the clock (one step per `TRACK_STEP`, `track_service::steps_once_a_second`):
/// frames, however many, push the input once and step nothing themselves.
#[test]
fn track_step_once_per_second() {
    let rec = Rec::default();
    let mut st = state(settings(), &rec);
    st.track_sync();
    st.track_sync();
    st.track_sync();
    assert_eq!(rec.0.borrow().inputs.len(), 1);
    assert!(rec.0.borrow().steps.is_empty());
    rec.worker_step(t0());
    st.track_sync();
    assert_eq!(rec.0.borrow().steps.len(), 1);
    assert_eq!(rec.0.borrow().inputs.len(), 1);
}

#[test]
fn track_sync_pushes_on_change_only() {
    let rec = Rec::default();
    let mut st = state(settings(), &rec);
    st.track_sync();
    st.track_sync();
    assert_eq!(rec.0.borrow().inputs.len(), 1);
    st.select_plane("TST123");
    st.track_sync();
    let l = rec.0.borrow();
    assert_eq!(l.inputs.len(), 2);
    assert_eq!(l.inputs[1].hex.as_deref(), Some("TST123"));
    assert_eq!(l.inputs[1].rev, l.inputs[0].rev + 1);
}

fn shown(rev: u64, status: TrackStatus) -> TrackShown {
    let mut v = TrackValues::new(t0());
    v.hex = "TST123".into();
    TrackShown {
        rev,
        values: Some(v),
        status,
    }
}

#[test]
fn stale_shown_revision_ignored() {
    let rec = Rec::default();
    let mut st = state(settings(), &rec);
    st.play().unwrap();
    st.select_plane("TST123");
    st.track_sync();
    st.pause();
    st.track_sync();
    let rev = rec.0.borrow().inputs.last().unwrap().rev;
    assert_eq!(st.track_status, TrackStatus::Stopped);
    rec.0.borrow_mut().shown = Some(shown(rev - 1, TrackStatus::Tracking));
    st.track_sync();
    assert_eq!(st.track_status, TrackStatus::Stopped);
    assert!(st.track.is_none());
}

#[test]
fn shown_applied_when_revision_matches() {
    let rec = Rec::default();
    let mut st = state(settings(), &rec);
    st.play().unwrap();
    st.select_plane("TST123");
    st.track_sync();
    let rev = rec.0.borrow().inputs.last().unwrap().rev;
    let sh = shown(rev, TrackStatus::Tracking);
    rec.0.borrow_mut().shown = Some(sh.clone());
    st.track_sync();
    assert_eq!(st.track_status, TrackStatus::Tracking);
    // values that are not computed are NaN: compare what identifies them
    let id = |v: &Option<TrackValues>| v.as_ref().map(|v| (v.hex.clone(), v.timestamp));
    assert_eq!(id(&st.track), id(&sh.values));
    // ERROR of a switched-off output holds until the input or the worker's answer changes
    st.start_integrations();
    let g = st.integration_gen;
    st.on_event(track(g, TrackEvent::Disabled(OutputGroup::Udp)));
    assert_eq!(st.track_status, TrackStatus::Error);
    st.track_sync();
    assert_eq!(st.track_status, TrackStatus::Error);
    let mut next = shown(rev, TrackStatus::Tracking);
    next.values.as_mut().unwrap().timestamp = t0() + chrono::Duration::seconds(1);
    rec.0.borrow_mut().shown = Some(next);
    st.track_sync();
    assert_eq!(st.track_status, TrackStatus::Tracking);
}

#[test]
fn no_steps_without_activate_or_in_history() {
    let rec = Rec::default();
    let mut s = settings();
    s.track.activate = false;
    let mut st = state(s, &rec);
    st.right_click_dx();
    sync_step(&mut st, &rec);
    assert!(rec.0.borrow().steps.is_empty());
    assert_eq!(rec.0.borrow().inputs.last().unwrap().params, None);
    st.settings.track.activate = true;
    st.enter_history().unwrap();
    st.right_click_dx();
    sync_step(&mut st, &rec);
    assert!(rec.0.borrow().steps.is_empty());
    assert!(!rec.0.borrow().inputs.last().unwrap().life);
}

#[test]
fn nearest_settings_pushed_on_change() {
    let rec = Rec::default();
    let mut s = settings();
    s.server.activate = true;
    let mut st = state(s, &rec);
    st.sync_integrations();
    st.sync_integrations();
    assert_eq!(rec.0.borrow().nearest.len(), 1);
    assert_eq!(
        rec.0.borrow().nearest[0],
        NearestSettings::from_settings(&st.settings)
    );
    let other = if st.settings.band == Band::B432M {
        Band::B144M
    } else {
        Band::B432M
    };
    st.settings.band = other;
    st.sync_integrations();
    assert_eq!(rec.0.borrow().nearest.len(), 2);
    assert_eq!(rec.0.borrow().nearest[1].band, other);
    assert_eq!(
        st.nearest_synced,
        Some(NearestSettings::from_settings(&st.settings))
    );
    // a start hands the services the settings: no extra push
    st.start_integrations();
    st.sync_integrations();
    assert_eq!(rec.0.borrow().nearest.len(), 2);
    // without the receiver nothing is pushed
    st.settings.server.activate = false;
    st.settings.band = Band::B10G;
    st.settings.planes_position_ttl_min += 1;
    st.sync_integrations();
    assert_eq!(rec.0.borrow().nearest.len(), 2);
}

/// Waits up to `limit` for `f` to give `Some`, returning early.
fn wait_for<T>(limit: Duration, mut f: impl FnMut() -> Option<T>) -> Option<T> {
    let end = Instant::now() + limit;
    loop {
        if let Some(v) = f() {
            return Some(v);
        }
        if Instant::now() > end {
            return None;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn real_services_restart_rebinds_new_port() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = std::sync::mpsc::channel();
    let mut svc =
        RealServices::new(open_data(dir.path()).unwrap(), tx).with_net(NetAddrs::loopback());
    let mut s = settings();
    s.track.activate = false;
    s.server.activate = true;
    s.server.port = 0;
    let cache = Arc::new(PlaneInfoCache::new());
    svc.start_integrations(&s, 1, cache.clone());
    let a1: SocketAddr =
        wait_for(Duration::from_secs(10), || svc.wintest_local_addr()).expect("first bind");
    assert_eq!(a1.ip(), std::net::Ipv4Addr::LOCALHOST);
    assert_ne!(a1.port(), 0);
    svc.start_integrations(&s, 2, cache);
    let a2 = wait_for(Duration::from_secs(10), || svc.wintest_local_addr()).expect("second bind");
    // The sockets reuse the address, so the second port-0 bind may land on a1 again; then the
    // new receiver holds a1 and the freed check cannot tell anything.
    if a2 != a1 {
        // the old receiver let go of its port
        let freed = wait_for(Duration::from_secs(3), || UdpSocket::bind(a1).ok());
        assert!(freed.is_some(), "{a1} still bound after the restart");
    }
    svc.stop_integrations();
    assert_eq!(svc.wintest_local_addr(), None);
}

#[test]
fn real_services_track_writes_azel() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = std::sync::mpsc::channel();
    let mut svc =
        RealServices::new(open_data(dir.path()).unwrap(), tx).with_net(NetAddrs::loopback());
    let mut s = settings();
    s.server.activate = false;
    s.track.udp = TrackUdp::None;
    s.track.file = TrackFile::Native;
    s.track.set_az = 10.0;
    s.track.set_el = 1.0;
    svc.start_integrations(&s, 1, Arc::new(PlaneInfoCache::new()));
    // SINGLE points at `Track_SetAz`/`Track_SetEl`; the worker steps on its own
    svc.track_input(TrackInput {
        rev: 1,
        life: true,
        mode: TrackMode::Single,
        hex: None,
        heights: None,
        params: Some(TrackParams::from_settings(&s)),
    });
    let file = dir.path().join("Tmp").join("azel.dat");
    let want = format!("10.0,1.0{NEWLINE}");
    let got = wait_for(Duration::from_secs(10), || {
        std::fs::read_to_string(&file).ok().filter(|t| *t == want)
    });
    assert_eq!(got.as_deref(), Some(want.as_str()));
    svc.stop_integrations();
}

#[test]
fn status_bar_shows_rot_after_trk() {
    let rec = Rec::default();
    let mut st = state(settings(), &rec);
    st.rot_status = RotStatus::Tracking;
    let mut h = Harness::new_ui_state(
        |ui, st: &mut AppState| airscout::ui::status::show(ui, st),
        st,
    );
    h.run();
    let trk = h.get_by_label("TRK").rect().min.x;
    let rot = h.get_by_label("ROT").rect().min.x;
    assert!(rot > trk, "ROT at {rot}, TRK at {trk}");
}

#[test]
fn sync_pushes_web_settings_once_per_second() {
    let rec = Rec::default();
    let mut s = settings();
    s.server.activate = true;
    let mut st = state(s, &rec);
    // the start hands the services the settings and starts the second
    st.start_integrations();
    assert_eq!(
        st.web_synced.as_ref().map(|(_, s)| s.my_call.as_str()),
        Some("TE1ST")
    );
    let t = Instant::now() + Duration::from_secs(1);
    st.settings.my_call = "TE2ND".into();
    st.sync_integrations_at(t);
    // two frames within one second, the call changed again: one push
    st.settings.my_call = "TE1ST".into();
    st.sync_integrations_at(t + Duration::from_millis(500));
    assert_eq!(rec.0.borrow().web, vec!["TE2ND".to_string()]);
    // one second later the newer settings go out
    st.sync_integrations_at(t + Duration::from_secs(1));
    assert_eq!(
        rec.0.borrow().web,
        vec!["TE2ND".to_string(), "TE1ST".to_string()]
    );
    // unchanged settings are not pushed again
    st.sync_integrations_at(t + Duration::from_secs(3));
    assert_eq!(rec.0.borrow().web.len(), 2);
    assert_eq!(
        st.web_synced.as_ref().map(|(_, s)| s.my_call.as_str()),
        Some("TE1ST")
    );
}

#[test]
fn no_web_sync_when_server_off() {
    let rec = Rec::default();
    let mut st = state(settings(), &rec);
    st.start_integrations();
    let t = Instant::now() + Duration::from_secs(1);
    st.settings.my_call = "TE2ND".into();
    st.sync_integrations_at(t);
    st.sync_integrations_at(t + Duration::from_secs(2));
    st.sync_integrations();
    assert!(rec.0.borrow().web.is_empty());
}
