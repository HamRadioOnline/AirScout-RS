//! The worker configuration of the app settings (feature `settings`). The app and `catcheck`
//! share it, so both apply the same clamps.

use std::time::Duration;

use as_settings::{CatEngine, Settings};

use crate::{CatConfig, Engine, SerialConfig};

/// Range of `CAT_Update` and `CAT_Timeout` in the Options dialog [ms]
/// (`OptionsDlg.Designer.cs` `ud_Options_CAT_Update`/`ud_Options_CAT_Timeout`).
const MIN_MS: u64 = 100;
const MAX_MS: u64 = 10_000;

fn parity(p: as_settings::Parity) -> crate::Parity {
    use as_settings::Parity as P;
    match p {
        P::None => crate::Parity::None,
        P::Odd => crate::Parity::Odd,
        P::Even => crate::Parity::Even,
        P::Mark => crate::Parity::Mark,
        P::Space => crate::Parity::Space,
    }
}

fn stop_bits(s: as_settings::StopBits) -> crate::StopBits {
    use as_settings::StopBits as S;
    match s {
        S::One => crate::StopBits::One,
        S::One5 => crate::StopBits::One5,
        S::Two => crate::StopBits::Two,
    }
}

fn flow(f: as_settings::FlowControl) -> crate::FlowControl {
    use as_settings::FlowControl as F;
    match f {
        F::Low => crate::FlowControl::Low,
        F::High => crate::FlowControl::High,
        F::Handshake => crate::FlowControl::Handshake,
    }
}

/// The worker configuration of the settings. Update and timeout are clamped to the Options
/// range, so a hand-edited file cannot make the worker spin or hang.
pub fn cat_config(s: &Settings) -> CatConfig {
    let c = &s.cat;
    let engine = match c.engine {
        CatEngine::None => Engine::None,
        CatEngine::Rigctld => Engine::Rigctld {
            host: c.rigctld_host.clone(),
            port: c.rigctld_port,
        },
        CatEngine::Hamlib => Engine::Hamlib {
            model: c.hamlib_model,
            serial: SerialConfig {
                port_name: c.port_name.clone(),
                baudrate: c.baudrate,
                data_bits: c.data_bits,
                parity: parity(c.parity),
                stop_bits: stop_bits(c.stop_bits),
                rts: flow(c.rts),
                dtr: flow(c.dtr),
            },
        },
        CatEngine::OmniRig => Engine::OmniRig {
            rig: c.omnirig_rig.clamp(1, 4),
        },
    };
    let ms = |v: u64| Duration::from_millis(v.clamp(MIN_MS, MAX_MS));
    CatConfig {
        engine,
        update: ms(c.update_ms),
        timeout: ms(c.timeout_ms),
    }
}
