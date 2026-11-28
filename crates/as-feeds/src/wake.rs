//! Wake-turbulence category mapping shared by the JSON feeds (`ADSBOne.cs:587-601`).

/// ADS-B emitter category (`A1`..`A5`, case-sensitive) to `PLANECATEGORY` number:
/// A1/A2 light (1), A3/A4 medium (2), A5 heavy (3), anything else 0. Type `A388` is always
/// superheavy (4).
pub fn category(emitter: &str, type_code: &str) -> i32 {
    let c = match emitter {
        "A1" | "A2" => 1,
        "A3" | "A4" => 2,
        "A5" => 3,
        _ => 0,
    };
    if type_code == "A388" {
        4
    } else {
        c
    }
}
