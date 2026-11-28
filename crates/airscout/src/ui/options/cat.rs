//! CAT tab (`OptionsDlg` CAT tab, `:2744-3080`): activation, engine and rig settings.

use super::{num, num_step, text};
use as_cat::{hamlib_available, omnirig_available};
use as_settings::{CatEngine, Settings};
use eframe::egui::{ComboBox, Grid, Ui};

/// Engines the list offers. The native library only shows in a build that has it.
pub fn engines() -> Vec<(CatEngine, &'static str)> {
    let mut v = vec![
        (CatEngine::None, "[None]"),
        (CatEngine::Rigctld, "[rigctld] Hamlib rigctld (TCP)"),
    ];
    if omnirig_available() {
        v.push((
            CatEngine::OmniRig,
            "[OmniRig] OmniRig V1.19 or V2.x (ActiveX)",
        ));
    }
    if hamlib_available() {
        v.push((CatEngine::Hamlib, "[Hamlib] native library"));
    }
    v
}

/// Reasons the CAT tab cannot be applied. Only an active rigctld engine has fields to check.
pub fn validate(s: &Settings) -> Vec<String> {
    let mut e = Vec::new();
    let c = &s.cat;
    if c.activate && c.engine == CatEngine::Rigctld {
        if c.rigctld_host.trim().is_empty() {
            e.push("CAT: the rigctld host must not be empty.".to_string());
        }
        if c.rigctld_port == 0 {
            e.push("CAT: the rigctld port must be between 1 and 65535.".to_string());
        }
    }
    e
}

pub fn show(ui: &mut Ui, s: &mut Settings) {
    ui.group(|ui| {
        ui.strong("Activate CAT Interface");
        ui.checkbox(&mut s.cat.activate, "Activate CAT Interface");
        ui.add_enabled_ui(s.cat.activate, |ui| {
            Grid::new("opt_cat_update").num_columns(2).show(ui, |ui| {
                num_step(
                    ui,
                    "CAT Refresh [ms]:",
                    &mut s.cat.update_ms,
                    100..=10000,
                    100.0,
                );
            });
        });
    });
    ui.add_enabled_ui(s.cat.activate, |ui| {
        rig_type(ui, s);
        if s.cat.engine == CatEngine::Rigctld {
            rigctld(ui, s);
        }
        if s.cat.engine == CatEngine::OmniRig && omnirig_available() {
            omnirig(ui, s);
        }
        #[cfg(feature = "hamlib")]
        if s.cat.engine == CatEngine::Hamlib {
            serial::show(ui, s);
        }
        ui.group(|ui| {
            ui.strong("Operating Instructions");
            ui.label(
                "CAT control is introduced for Doppler shift compensation (see \"Track\" tab for \
                 details). \nAirScout is calculating tracking and Doppler shifts once per second \
                 by default. Each communication with rig must be completed within this period.",
            );
            if cfg!(windows) {
                ui.label(
                    "  The  recommended settings for OmniRig are:\n\n- Polling interval:               \
                     100ms ...500ms\n- Timeout:                           500ms .. 1sec \
                     (essential for OmniRig)",
                );
            }
        });
    });
}

fn rig_type(ui: &mut Ui, s: &mut Settings) {
    ui.group(|ui| {
        ui.strong("Rig Type");
        let list = engines();
        ui.horizontal(|ui| {
            let l = ui.label("Select CAT-Engine & Rig-Type:");
            let shown = list
                .iter()
                .find(|x| x.0 == s.cat.engine)
                .map_or(unavailable(s.cat.engine), |x| x.1);
            ComboBox::from_id_salt("opt_cat_engine")
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    for (e, name) in &list {
                        ui.selectable_value(&mut s.cat.engine, *e, *name);
                    }
                })
                .response
                .labelled_by(l.id);
        });
        ui.label("AirScout supports the following CAT-Engines:");
        let mut n = 1;
        if omnirig_available() {
            ui.label(
                "1. OmniRig V1.19 or V2.x via ActiveX (Windows only)\n(multiple access to rig \
                 from different applications possible)",
            );
            n += 1;
        }
        ui.label(format!(
            "{n}. Hamlib rigctld via TCP (all systems)\n(rigctld must be running; several \
             programs can share the rig through it)"
        ));
        if hamlib_available() {
            ui.label(format!(
                "{}. Hamlib native library (needs libhamlib, exclusive access to the rig)",
                n + 1
            ));
        }
        ui.label(
            "Rig must support a minimum of CAT control capabilities:\n- set frequency of VFO A \
             and VFO B\n- set/reset Split Mode",
        );
    });
}

/// What the engine box shows for an engine this build does not offer.
fn unavailable(e: CatEngine) -> &'static str {
    match e {
        CatEngine::OmniRig => "[OmniRig] not available on this system",
        _ => "[Hamlib] not available in this build",
    }
}

/// The text of OmniRig rig `rig`, clamped to 1..=4 as `cat_config` does, so a hand-edited
/// setting shows the rig the CAT worker uses.
pub fn omnirig_rig_text(rig: u8) -> String {
    format!("[OmniRig] Rig {}", rig.clamp(1, 4))
}

/// OmniRig group: only the rig choice, the original disables port, baudrate and timeout for
/// this engine (`:2878-2896`).
fn omnirig(ui: &mut Ui, s: &mut Settings) {
    ui.group(|ui| {
        ui.strong("OmniRig Settings");
        ui.horizontal(|ui| {
            let l = ui.label("Rig:");
            ComboBox::from_id_salt("opt_cat_omnirig")
                .selected_text(omnirig_rig_text(s.cat.omnirig_rig))
                .show_ui(ui, |ui| {
                    for n in 1..=4u8 {
                        ui.selectable_value(&mut s.cat.omnirig_rig, n, omnirig_rig_text(n));
                    }
                })
                .response
                .labelled_by(l.id);
        });
        ui.label("Rig 3 and Rig 4 need OmniRig V2.x.");
    });
}

fn rigctld(ui: &mut Ui, s: &mut Settings) {
    ui.group(|ui| {
        ui.strong("rigctld Settings");
        Grid::new("opt_cat_rigctld").num_columns(2).show(ui, |ui| {
            text(ui, "Host:", &mut s.cat.rigctld_host);
            num(ui, "Port:", &mut s.cat.rigctld_port, 0..=65535);
            num_step(
                ui,
                "Timeout [ms]:",
                &mut s.cat.timeout_ms,
                100..=10000,
                100.0,
            );
        });
        ui.label(
            "rigctld is reached over plain TCP without authentication; keep it on this computer \
             or a trusted network.",
        );
    });
}

/// Serial port group of the native hamlib engine.
#[cfg(feature = "hamlib")]
mod serial {
    use super::{num, num_step, text};
    use as_settings::{FlowControl, Parity, Settings, StopBits};
    use eframe::egui::{ComboBox, Grid, Ui};

    const BAUDRATES: [u32; 15] = [
        110, 300, 600, 1200, 2400, 4800, 9600, 14400, 19200, 38400, 56000, 57600, 115200, 128000,
        256000,
    ];
    const DATA_BITS: [u8; 4] = [5, 6, 7, 8];
    const PARITIES: [(Parity, &str); 5] = [
        (Parity::None, "None"),
        (Parity::Odd, "Odd"),
        (Parity::Even, "Even"),
        (Parity::Mark, "Mark"),
        (Parity::Space, "Space"),
    ];
    const STOP_BITS: [(StopBits, &str); 3] = [
        (StopBits::One, "1"),
        (StopBits::One5, "1,5"),
        (StopBits::Two, "2"),
    ];
    const FLOW: [(FlowControl, &str); 3] = [
        (FlowControl::Low, "Low"),
        (FlowControl::High, "High"),
        (FlowControl::Handshake, "Handshake"),
    ];

    fn pick<T: PartialEq + Copy>(ui: &mut Ui, label: &str, value: &mut T, items: &[(T, String)]) {
        let l = ui.label(label);
        let shown = items.iter().find(|x| x.0 == *value).map_or("", |x| &x.1);
        ComboBox::from_id_salt(label)
            .selected_text(shown)
            .show_ui(ui, |ui| {
                for (v, name) in items {
                    ui.selectable_value(value, *v, name);
                }
            })
            .response
            .labelled_by(l.id);
        ui.end_row();
    }

    fn named<T: Copy>(items: &[(T, &str)]) -> Vec<(T, String)> {
        items.iter().map(|(v, n)| (*v, n.to_string())).collect()
    }

    pub fn show(ui: &mut Ui, s: &mut Settings) {
        ui.group(|ui| {
            ui.strong("Serial Port Settings");
            let c = &mut s.cat;
            Grid::new("opt_cat_serial").num_columns(2).show(ui, |ui| {
                num(ui, "Hamlib Rig Model:", &mut c.hamlib_model, 0..=u32::MAX);
                text(ui, "PortName:", &mut c.port_name);
                let bauds: Vec<_> = BAUDRATES.iter().map(|b| (*b, b.to_string())).collect();
                pick(ui, "Baudrate:", &mut c.baudrate, &bauds);
                let bits: Vec<_> = DATA_BITS.iter().map(|b| (*b, b.to_string())).collect();
                pick(ui, "Databits:", &mut c.data_bits, &bits);
                pick(ui, "Parity:", &mut c.parity, &named(&PARITIES));
                pick(ui, "Stopbits:", &mut c.stop_bits, &named(&STOP_BITS));
                pick(ui, "RTS:", &mut c.rts, &named(&FLOW));
                pick(ui, "DTR:", &mut c.dtr, &named(&FLOW));
                num_step(ui, "Timeout [ms]:", &mut c.timeout_ms, 100..=10000, 100.0);
            });
        });
    }
}
