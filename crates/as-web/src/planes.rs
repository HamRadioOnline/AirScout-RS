//! The `planes.json` writer shared by the app and `airscout-planeserver`
//! (`MapDlg.cs:8481-8528`): the FR24 layout.

use as_aircraft::PlaneInfo;
use as_wintest::dotnet::fixed;
use chrono::DateTime;

/// The `version` member of `planes.json`. The AirScoutServer feed reads the FR24 branch
/// only when the text contains `"version":1` (`AirScoutServer.cs:353`). The C# writer
/// (`AirScout/MapDlg.cs:8481`) writes the assembly major version, and the assembly version is
/// `1.4.5.0` (`AirScout/Properties/AssemblyInfo.cs:35`), major 1. The AirScout-RS major
/// version is 0, which the feed would not read (finding 16, amended; finding 269).
pub const PLANES_JSON_VERSION: u64 = 1;

/// `s` as a JSON string. C# writes the text between quotes unchanged (L19); for a value
/// without `"`, `\` or control characters the text is the same.
fn quoted(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into())
}

/// The `planes.json` text of `planes` (`MapDlg.cs:8481-8528`): a header, one line per
/// plane, a closing line. `version` is the `version` member.
pub fn planes_json(planes: &[PlaneInfo], version: u64) -> String {
    let mut out = format!("{{\"full_count\":{},\"version\":{version}", planes.len());
    for (i, p) in planes.iter().enumerate() {
        // `(long)(Time - 1970).TotalSeconds` truncates toward zero, as `num_seconds` does
        let time = (p.time - DateTime::UNIX_EPOCH).num_seconds();
        out.push_str(&format!(
            ",\"{i:08x}\":[{},{},{},{},{},{},\"\",\"\",{},{},{time},\"\",\"\",\"\",0,0,{},0]\n",
            quoted(&p.hex),
            fixed(p.lat, 4),
            fixed(p.lon, 4),
            fixed(p.track, 0),
            fixed(p.alt_ft, 0),
            fixed(p.speed_kts, 0),
            quoted(&p.type_code),
            quoted(&p.reg),
            quoted(&p.call),
        ));
    }
    out.push_str("}\n");
    out
}
