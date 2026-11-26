//! Alarm sound: the [`Beeper`] behind a trait (tests use a fake), and the bookkeeping that
//! beeps once per tick in which at least one plane starts to alarm.
//!
//! C# plays `SystemSounds.Beep` on every update that has an alarm plane (MapDlg.cs:3485); the
//! port beeps once when planes start to alarm instead.

use std::collections::BTreeSet;

/// Plays one alarm beep.
pub trait Beeper {
    fn beep(&mut self);
}

/// Silent: used when the `sound` feature is off or no audio device exists.
pub struct NullBeeper;

impl Beeper for NullBeeper {
    fn beep(&mut self) {}
}

/// Frequency and length of the generated beep.
pub const BEEP_HZ: f32 = 880.0;
pub const BEEP_MS: u64 = 300;

#[cfg(feature = "sound")]
pub use rodio_beeper::RodioBeeper;

#[cfg(feature = "sound")]
mod rodio_beeper {
    use super::{Beeper, BEEP_HZ, BEEP_MS};
    use rodio::source::SineWave;
    use rodio::{DeviceSinkBuilder, MixerDeviceSink, Source};
    use std::time::Duration;

    /// A generated sine beep on the default audio output.
    pub struct RodioBeeper {
        sink: MixerDeviceSink,
    }

    impl RodioBeeper {
        /// Open the default output; the error text says why there is none.
        pub fn new() -> Result<Self, String> {
            let mut sink = DeviceSinkBuilder::open_default_sink().map_err(|e| e.to_string())?;
            sink.log_on_drop(false);
            Ok(Self { sink })
        }
    }

    impl Beeper for RodioBeeper {
        fn beep(&mut self) {
            let wave = SineWave::new(BEEP_HZ)
                .amplify(0.2)
                .take_duration(Duration::from_millis(BEEP_MS));
            self.sink.mixer().add(wave);
        }
    }
}

/// The best beeper available and, when there is none, the one status message to show.
pub fn default_beeper() -> (Box<dyn Beeper>, Option<String>) {
    #[cfg(feature = "sound")]
    {
        match RodioBeeper::new() {
            Ok(b) => (Box::new(b), None),
            Err(e) => (
                Box::new(NullBeeper),
                Some(format!("no audio output, the alarm is silent: {e}")),
            ),
        }
    }
    #[cfg(not(feature = "sound"))]
    {
        (Box::new(NullBeeper), None)
    }
}

/// The hexes that alarmed in the last tick.
#[derive(Default)]
pub struct AlarmBeeps {
    alarmed: BTreeSet<String>,
}

impl AlarmBeeps {
    /// One beep if any hex in `alarming` did not alarm in the previous tick, if `enabled`: planes
    /// that start to alarm in the same tick share one beep (stacked beeps would add up and clip).
    /// A plane that stops alarming and starts again beeps again.
    pub fn update(&mut self, alarming: &[String], enabled: bool, beeper: &mut dyn Beeper) {
        let now: BTreeSet<String> = alarming.iter().cloned().collect();
        if enabled && now.difference(&self.alarmed).next().is_some() {
            beeper.beep();
        }
        self.alarmed = now;
    }

    pub fn clear(&mut self) {
        self.alarmed.clear();
    }
}
