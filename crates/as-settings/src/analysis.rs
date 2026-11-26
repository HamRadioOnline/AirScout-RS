//! History / crossing analysis settings (stage 3c). Defaults from `Settings.settings:389-404, :523-526`.

use chrono::NaiveDate;

#[derive(Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Analysis {
    /// History archive URL. No default: the user supplies it.
    pub history_url: String,
    pub history_disclaimer_accepted: bool,
    /// Empty means `<data>/Tmp/History`.
    pub history_directory: String,
    pub history_date: NaiveDate,
    /// History replay step [min].
    pub history_step_min: u32,
    pub load_file_name: String,
    pub crossing_with_signal_level: bool,
    /// Gap [s] above which a crossing counts as ambiguous.
    pub crossing_ambiguous_gap_s: u32,
}

impl Default for Analysis {
    fn default() -> Self {
        Self {
            history_url: String::new(),
            history_disclaimer_accepted: false,
            history_directory: String::new(),
            history_date: NaiveDate::from_ymd_opt(2017, 7, 1).unwrap(),
            history_step_min: 5,
            load_file_name: String::new(),
            crossing_with_signal_level: false,
            crossing_ambiguous_gap_s: 10,
        }
    }
}

impl std::fmt::Debug for Analysis {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Analysis")
            .field(
                "history_disclaimer_accepted",
                &self.history_disclaimer_accepted,
            )
            .field("history_directory", &self.history_directory)
            .field("history_date", &self.history_date)
            .field("history_step_min", &self.history_step_min)
            .field("load_file_name", &self.load_file_name)
            .field(
                "crossing_with_signal_level",
                &self.crossing_with_signal_level,
            )
            .field("crossing_ambiguous_gap_s", &self.crossing_ambiguous_gap_s)
            .finish_non_exhaustive()
    }
}
