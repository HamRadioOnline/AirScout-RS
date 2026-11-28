//! Track tab (`OptionsDlg` Track tab, `:2583-2742`): tracking on or off, the time offset, the
//! UDP, serial (GS-232A), DDE (Windows only) and `azel.dat` outputs of the Track worker and the
//! Doppler strategy. The Doppler group needs the activated CAT interface (CAT tab).

use super::{num, text};
#[cfg(windows)]
use as_settings::TrackDde;
use as_settings::{DopplerStrategy, Settings, TrackFile, TrackSerial, TrackUdp};
use eframe::egui::{ComboBox, Grid, Ui};

const STRATEGIES: [(DopplerStrategy, &str); 5] = [
    (DopplerStrategy::None, "None"),
    (DopplerStrategy::A, "A: RX shifted, TX on dial"),
    (DopplerStrategy::B, "B: RX on dial, TX shifted"),
    (DopplerStrategy::C, "C: RX and TX shifted"),
    (DopplerStrategy::D, "D: own Doppler only"),
];

/// Baudrates `tc_Track_Validating` accepts for the serial rotator (`:2670`).
pub const BAUDRATES: [u32; 14] = [
    50, 100, 110, 150, 300, 600, 1200, 2400, 4800, 9600, 19200, 38400, 57600, 115200,
];

/// Reasons the Track tab cannot be applied (`tc_Track_Validating`, `:2663-2678`, checked always).
pub fn validate(s: &Settings) -> Vec<String> {
    let mut e = Vec::new();
    if s.track.udp_wintest_port == s.track.udp_airscout_port {
        e.push("Track: Win-Test port and AirScout port must have different values.".to_string());
    }
    if !BAUDRATES.contains(&s.track.serial_baudrate) {
        e.push(format!(
            "Track: This is not a valid baudrate: {}\nValid baudrates are:\n\n50, 100, 110, 150, \
             300, 600, 1200, 2400, 4800, 9600, 19200, 38400, 57600, 115200",
            s.track.serial_baudrate
        ));
    }
    e
}

fn udp_row(ui: &mut Ui, udp: &mut TrackUdp, value: TrackUdp, label: &str, port: &mut u16) {
    ui.horizontal(|ui| {
        ui.radio_value(udp, value, label);
        let l = ui.label("Port:");
        ui.add(eframe::egui::DragValue::new(port).range(0..=65535))
            .labelled_by(l.id);
    });
}

pub fn show(ui: &mut Ui, s: &mut Settings) {
    ui.checkbox(&mut s.track.activate, "Activate Tracking");
    Grid::new("opt_track").num_columns(2).show(ui, |ui| {
        num(
            ui,
            "Tracking Time Offset: [s]",
            &mut s.track.offset_s,
            -3600..=3600,
        );
    });
    let t = &mut s.track;
    ui.add_enabled_ui(t.activate, |ui| {
        ui.group(|ui| {
            ui.strong("Rotator Control Network Output");
            ui.radio_value(&mut t.udp, TrackUdp::None, "None");
            udp_row(
                ui,
                &mut t.udp,
                TrackUdp::WinTest,
                "UDP Broadcast (Win-Test) Az only",
                &mut t.udp_wintest_port,
            );
            udp_row(
                ui,
                &mut t.udp,
                TrackUdp::AirScout,
                "UDP Broadcast (AirScout) Az/El",
                &mut t.udp_airscout_port,
            );
        });
        ui.group(|ui| {
            ui.strong("Rotator Control File Output");
            ui.radio_value(&mut t.file, TrackFile::None, "None");
            ui.radio_value(&mut t.file, TrackFile::Native, "Native Az/El");
            ui.radio_value(&mut t.file, TrackFile::Wsjt, "WSJT Az/El");
            ui.label("Writes azel.dat to the Tmp folder of the data directory.");
        });
        ui.group(|ui| {
            ui.strong("Rotator Control Serial Output");
            ui.radio_value(&mut t.serial, TrackSerial::None, "None");
            ui.radio_value(&mut t.serial, TrackSerial::Gs232Az, "GS-232A Az only");
            ui.radio_value(&mut t.serial, TrackSerial::Gs232AzEl, "GS-232A Az/El");
            Grid::new("opt_track_serial").num_columns(2).show(ui, |ui| {
                text(ui, "Port:", &mut t.serial_port);
                // `MinValue = 0`, `MaxValue = 115200` (`OptionsDlg.Designer.cs:7680-7681`)
                num(ui, "Baudrate:", &mut t.serial_baudrate, 0..=115_200);
            });
        });
        #[cfg(windows)]
        ui.group(|ui| {
            ui.strong("Rotator Control DDE Output");
            ui.radio_value(&mut t.dde, TrackDde::None, "None");
            ui.radio_value(
                &mut t.dde,
                TrackDde::Hrd,
                "Ham Radio Deluxe (HRDRotator.exe)",
            );
        });
    });
    // `cb_Options_Gauges_Show` lives in the Map tab's `groupBox22` (`ui/options/map.rs`).
    // `tab_Options_Track_Enter` (:2621-2628) and `cb_Options_CAT_Activate_CheckedChanged`
    // (:2896-2899): the group follows both activations.
    let enabled = s.cat.activate && s.track.activate;
    ui.add_enabled_ui(enabled, |ui| {
        ui.group(|ui| {
            ui.strong("Doppler Compensation (requires activated CAT and Rig connected)");
            ui.label(
                "You can see the dial frequency from rig here and choose how to compensate \
                 Doppler shift on your site and on DX site.",
            );
            Grid::new("opt_track_doppler")
                .num_columns(2)
                .show(ui, |ui| {
                    num(
                        ui,
                        "Dial frequency [Hz]",
                        &mut s.doppler.dial_freq_hz,
                        0..=as_cat::MAX_FREQ_HZ,
                    );
                    let l = ui.label("Doppler strategy");
                    let name = |d: DopplerStrategy| {
                        STRATEGIES.iter().find(|x| x.0 == d).map_or("", |x| x.1)
                    };
                    ComboBox::from_id_salt("opt_track_strategy")
                        .selected_text(name(s.doppler.strategy))
                        .show_ui(ui, |ui| {
                            for (d, text) in STRATEGIES {
                                ui.selectable_value(&mut s.doppler.strategy, d, text);
                            }
                        })
                        .response
                        .labelled_by(l.id);
                    ui.end_row();
                });
        });
    });
}
