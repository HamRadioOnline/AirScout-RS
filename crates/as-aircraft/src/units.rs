//! Port of `ScoutBase.Core/SupportFunctions.cs` `UnitConverter`.

/// Feet to metres.
pub fn ft_m(feet: f64) -> f64 {
    feet / 3.28084
}
/// Metres to feet.
pub fn m_ft(m: f64) -> f64 {
    m * 3.28084
}
/// Knots to km/h.
pub fn kts_kmh(kts: f64) -> f64 {
    kts * 1.852
}
/// km/h to knots.
pub fn kmh_kts(kmh: f64) -> f64 {
    kmh / 1.852
}
/// km to statute miles.
pub fn km_mi(km: f64) -> f64 {
    km / 1.609344
}
/// Statute miles to km.
pub fn mi_km(mi: f64) -> f64 {
    mi * 1.609344
}
