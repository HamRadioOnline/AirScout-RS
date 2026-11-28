use airscout::app::{AppState, MapTab, RealServices, Services};
use airscout::news::news_due;
use airscout::services::{open_data, PathRequest, ServiceEvent};
use as_aircraft::PlaneInfoCache;
use as_geo::maidenhead::{lat_lon_from_loc, PositionInRectangle};
use as_settings::Settings;
use chrono::{DateTime, TimeDelta, TimeZone, Utc};
use std::cell::RefCell;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::rc::Rc;
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

const URL: &str = "http://localhost/news";
const WAIT: Duration = Duration::from_secs(5);

#[derive(Clone, Default)]
struct RecordingServices(Rc<RefCell<Vec<String>>>);

impl Services for RecordingServices {
    fn request_path(&mut self, _: PathRequest) {}

    fn start_feed(&mut self, _: u64, _: &Settings, _: Arc<PlaneInfoCache>) -> Result<(), String> {
        Ok(())
    }

    fn stop_feed(&mut self) {}

    fn check_news(&mut self, url: &str) {
        self.0.borrow_mut().push(url.to_owned());
    }
}

fn date() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2015, 1, 31, 15, 21, 0).unwrap()
}

fn settings(url: &str, enabled: bool) -> Settings {
    let (my_lat, my_lon) = lat_lon_from_loc("JO50AA", PositionInRectangle::MiddleMiddle).unwrap();
    let mut settings = Settings {
        my_call: "TE1ST".into(),
        my_lat,
        my_lon,
        ..Settings::default()
    };
    settings.gui.news_url = url.into();
    settings.gui.news_feed_enabled = enabled;
    settings
}

fn state(settings: Settings, services: impl Services + 'static) -> AppState {
    AppState::new(
        settings,
        None,
        Box::new(services),
        Arc::new(PlaneInfoCache::new()),
    )
}

#[test]
fn news_due_rules() {
    let now = Instant::now();
    let interval = Duration::from_secs(3600);
    assert!(!news_due(None, now, 3600, false));
    assert!(!news_due(Some(now), now + interval, 3600, false));
    assert!(news_due(None, now, 3600, true));
    assert!(!news_due(
        Some(now),
        now + interval - Duration::from_millis(1),
        3600,
        true
    ));
    assert!(news_due(Some(now), now + interval, 3600, true));
}

#[test]
fn news_check_off_by_default() {
    let settings = Settings::default();
    assert!(!settings.gui.news_feed_enabled);
    let services = RecordingServices::default();
    let mut app = state(settings, services.clone());
    let now = Instant::now();
    app.poll_news(now);
    app.poll_news(now + Duration::from_secs(3600));
    assert!(services.0.borrow().is_empty());
}

#[test]
fn newer_page_opens_prompt() {
    let now = Instant::now();
    let interval = Duration::from_secs(3600);
    let newer = date() + TimeDelta::seconds(1);
    for yes in [true, false] {
        let services = RecordingServices::default();
        let mut app = state(settings(URL, true), services.clone());
        app.poll_news(now);
        app.on_event(ServiceEvent::News(Ok(newer)));
        assert_eq!(app.news_prompt, Some(newer));
        assert_eq!(app.settings.gui.news_last_update, date());

        app.poll_news(now + interval);
        assert_eq!(*services.0.borrow(), [URL]);
        assert_eq!(app.answer_news(yes).as_deref(), yes.then_some(URL));
        assert_eq!(app.map_tab, if yes { MapTab::News } else { MapTab::Map });
        assert_eq!(app.news_prompt, None);
        assert_eq!(app.settings.gui.news_last_update, newer);
        assert!(app.settings.gui.news_feed_enabled);
        app.poll_news(now + interval);
        assert_eq!(*services.0.borrow(), [URL, URL]);
        app.on_event(ServiceEvent::News(Ok(newer)));
        assert_eq!(app.news_prompt, None);

        app.poll_news(now + 2 * interval);
        assert_eq!(*services.0.borrow(), [URL, URL, URL]);
        let latest = newer + TimeDelta::seconds(1);
        app.on_event(ServiceEvent::News(Ok(latest)));
        assert_eq!(app.news_prompt, Some(latest));
    }
}

#[test]
fn confirming_options_preserves_news_answered_while_open() {
    let mut app = state(settings(URL, true), RecordingServices::default());
    app.poll_news(Instant::now());
    assert!(app.open_options_dlg());

    let newer = date() + TimeDelta::seconds(1);
    app.on_event(ServiceEvent::News(Ok(newer)));
    assert_eq!(app.news_prompt, Some(newer));
    app.answer_news(false);

    let draft = app.options_dlg.take().unwrap().draft;
    assert_eq!(draft.gui.news_last_update, date());
    app.apply_options(draft);
    app.options_closed();
    assert_eq!(app.settings.gui.news_last_update, newer);
}

#[test]
fn older_page_opens_nothing() {
    let services = RecordingServices::default();
    let mut app = state(settings(URL, true), services.clone());
    let now = Instant::now();
    let interval = Duration::from_secs(3600);
    app.poll_news(now);
    app.on_event(ServiceEvent::News(Ok(date() - TimeDelta::seconds(1))));
    assert_eq!(app.news_prompt, None);
    assert_eq!(app.settings.gui.news_last_update, date());

    app.poll_news(now + interval - Duration::from_millis(1));
    assert_eq!(*services.0.borrow(), [URL]);
    app.poll_news(now + interval);
    assert_eq!(*services.0.borrow(), [URL, URL]);
    app.on_event(ServiceEvent::News(Ok(date())));
    assert_eq!(app.news_prompt, None);
}

#[test]
fn error_goes_to_status_not_prompt() {
    let mut app = state(settings(URL, true), RecordingServices::default());
    app.poll_news(Instant::now());
    app.on_event(ServiceEvent::News(Err("synthetic failure".into())));
    assert_eq!(app.news_prompt, None);
    assert_eq!(app.settings.gui.news_last_update, date());
    let (timestamp, rest) = app.status.split_once("] ").unwrap();
    chrono::NaiveTime::parse_from_str(timestamp.strip_prefix('[').unwrap(), "%H:%M:%S").unwrap();
    assert_eq!(
        rest,
        format!("Error while reading the website {URL}: synthetic failure")
    );
}

#[test]
fn settings_without_news_keys_read_defaults() {
    let settings: Settings = toml::from_str("[gui]\nmap_update_s = 5\n").unwrap();
    assert_eq!(settings.gui.news_url, "http://airscout.eu/index.php/news");
    assert_eq!(settings.gui.news_interval_s, 3600);
    assert_eq!(settings.gui.news_last_update, date());
    assert!(!settings.gui.news_feed_enabled);
    assert_eq!(settings.gui.map_update_s, 5);
}

#[test]
fn check_runs_off_the_ui_thread() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/news", listener.local_addr().unwrap());
    let (request, observed) = mpsc::channel();
    let (release_headers, headers) = mpsc::channel();
    let (release_body, body) = mpsc::channel();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(WAIT)).unwrap();
        let mut reader = BufReader::new(&mut stream);
        let mut raw = String::new();
        loop {
            let mut line = String::new();
            assert_ne!(reader.read_line(&mut line).unwrap(), 0);
            if line == "\r\n" {
                break;
            }
            raw.push_str(&line);
        }
        request.send(raw).unwrap();
        headers.recv_timeout(WAIT).unwrap();
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nLast-Modified: Sat, 31 Jan 2015 15:21:01 GMT\r\nContent-Length: 1000000\r\n\r\n")
            .unwrap();
        body.recv_timeout(WAIT).unwrap();
    });

    let (events, inbox) = mpsc::channel();
    let (polled, returned) = mpsc::channel();
    let ui = std::thread::spawn(move || {
        let dir = tempfile::tempdir().unwrap();
        let services = RealServices::new(open_data(dir.path()).unwrap(), events);
        let mut app = state(settings(&url, true), services);
        let now = Instant::now();
        app.poll_news(now);
        app.poll_news(now + Duration::from_secs(3600));
        polled.send(()).unwrap();
        let event = inbox.recv_timeout(WAIT).unwrap();
        assert!(matches!(event, ServiceEvent::News(Ok(_))), "{event:?}");
        app.on_event(event);
        app.news_prompt
    });

    let raw = observed
        .recv_timeout(WAIT)
        .expect("expect a localhost request");
    returned
        .recv_timeout(WAIT)
        .expect("polling must return while the server holds the headers");
    assert!(raw.starts_with("GET /news HTTP/1.1\r\n"), "{raw}");
    assert!(
        raw.lines().any(|line| line.eq_ignore_ascii_case(&format!("user-agent: {}", as_net::user_agent()))),
        "{raw}"
    );
    assert!(!raw.contains("TE1ST") && !raw.contains("JO50AA"), "{raw}");
    release_headers.send(()).unwrap();
    assert_eq!(ui.join().unwrap(), Some(date() + TimeDelta::seconds(1)));
    release_body.send(()).unwrap();
    server.join().unwrap();
}
