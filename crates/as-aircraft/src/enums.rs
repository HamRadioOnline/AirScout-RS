//! Port of `AirScout.Core/Enums.cs`.

/// `AIRSCOUTPATHMODE`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PathMode {
    #[default]
    None = 0,
    Single = 1,
    Multi = 2,
}

/// `AIRSCOUTLIFEMODE`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LifeMode {
    #[default]
    None = 0,
    Life = 1,
    History = 2,
}

/// `AIRSCOUTPLAYMODE`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlayMode {
    #[default]
    None = 0,
    Pause = 1,
    FastBack = 2,
    Back = 3,
    Forward = 4,
    FastForward = 5,
}

/// `AIRSCOUTTRACKMODE`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TrackMode {
    #[default]
    None = 0,
    Single = 1,
    Track = 2,
    Error = 128,
}
