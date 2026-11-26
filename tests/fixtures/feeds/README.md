# Feed fixtures

Responses served by mockito in `crates/as-feeds/tests/json_feeds.rs`. Positions are around
locator JO50; no fixture contains user data.

| File | Source | Date |
|------|--------|------|
| `adsbone.json` | **Synthetic**, readsb v2 API format (`ac`, `now` in ms, `alt_baro`, `seen_pos`, `desc`, …). A real response could not be recorded: `api.adsb.one/v2/point/50.5/11.0/100` answered HTTP 403 (Cloudflare challenge) to the honest `AirScout-RS/0.1.0` User-Agent. Contains one `"alt_baro":"ground"` entry (added by hand), one without `seen_pos`, one TIS-B `~` hex, and a `desc` without a space. | 2026-11-23 |
| `readsb_aircraft.json` | Synthetic, readsb `aircraft.json` format (`now` in s, `alt_baro`, `gs`). One ground aircraft, one without position, categories A1-A5. | 2026-11-23 |
| `dump1090_aircraft.json` | Synthetic, dump1090(-mutability) `aircraft.json` format (`altitude`, `speed`). Same cases as readsb. | 2026-11-23 |
| `airscoutserver_planes.json` | Synthetic `List<PlaneJSON>` as AirScout serves it; one entry with explicit nulls, one with missing keys. | 2026-11-23 |
| `airscoutserver_fr24.json` | Synthetic legacy FR24-style dictionary (`full_count`, `version`, one array per aircraft); one key not starting with `0`. | 2026-11-23 |
| `opensky_states.json` | **Recorded** anonymous `https://opensky-network.org/api/states/all?lamin=50&lomin=10&lamax=51&lomax=12` with the `AirScout-RS/0.1.0` User-Agent (5 states). Added by hand: an `on_ground` state (`3c66b3`), a null callsign (`4b1a2c`), a null `time_position` (`3e1c40`), a zero velocity (`3d0a11`) and a short state array (`3c0001`). | 2026-11-23 |
| `adsbsupport_planes.json` | Synthetic `{planes:[...]}` in the shape of the ADSBSupport plugin DTO; one entry without `receivedAt`. | 2026-11-23 |
| `vrs_aircraftlist.json` | Synthetic Virtual Radar Server `AircraftList.json`; one aircraft without `PosTime`, WTC 0-3, `lastDv`. | 2026-11-23 |
| `flexjson_array_of_arrays.json` | Synthetic top-level array of 12 arrays `[hex, call, lat, lon, alt, track, speed, type, reg, vspeed]` for the FlexJSON column detection. | 2026-11-23 |
| `flexjson_vrs_like.json` | Synthetic VRS-like `{acList:[{...}]}` with 12 objects of 10 members; fractional `Lat`/`Long` (dropped by FlexJSON's dictionary branch). | 2026-11-23 |
| `flexjson_nested.json` | Synthetic nested `{meta, data:{aircraft:{<hex>:[lat, lon, alt, track, speed, type, reg, call]}}}`; the hex is only the key. | 2026-11-23 |

`tests/golden/feed_json.json` records what Newtonsoft.Json makes of the readsb and dump1090
fixtures with the original plugin DTOs (with and without the ground aircraft).
