//! Port of `AirScout.Core/PlaneCategory.cs`.

use serde::{Deserialize, Serialize};

/// Wake turbulence category (`PLANECATEGORY`).
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub enum PlaneCategory {
    #[default]
    None = 0,
    Light = 1,
    Medium = 2,
    Heavy = 3,
    Superheavy = 4,
}

impl PlaneCategory {
    /// All values, `None` first (C# `GetValues`).
    pub const ALL: [PlaneCategory; 5] = [
        PlaneCategory::None,
        PlaneCategory::Light,
        PlaneCategory::Medium,
        PlaneCategory::Heavy,
        PlaneCategory::Superheavy,
    ];

    /// Enum from its integer value.
    pub fn from_i32(v: i32) -> Option<Self> {
        Self::ALL.into_iter().find(|c| *c as i32 == v)
    }

    /// `GetStringValue`.
    pub fn as_str(self) -> &'static str {
        match self {
            PlaneCategory::None => "None",
            PlaneCategory::Light => "Light",
            PlaneCategory::Medium => "Medium",
            PlaneCategory::Heavy => "Heavy",
            PlaneCategory::Superheavy => "Superheavy",
        }
    }

    /// `GetShortStringValue`: first letter.
    pub fn short(self) -> &'static str {
        &self.as_str()[..1]
    }

    /// `ParseStringValue`: exact (case-sensitive) match of a value except `None`, else `None`.
    pub fn parse(s: &str) -> Self {
        Self::ALL[1..]
            .iter()
            .copied()
            .find(|c| c.as_str() == s)
            .unwrap_or(PlaneCategory::None)
    }
}
