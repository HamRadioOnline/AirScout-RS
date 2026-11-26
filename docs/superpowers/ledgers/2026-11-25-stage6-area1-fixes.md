# Stage 6 area 1 implementation ledger

[Implementation plan](../plans/2026-11-25-stage6-area1-fixes.md) · [Design](../specs/2026-11-25-stage6-logic-fixes-design.md) · [Finding-by-finding upstream records and golden deltas](../../logic-fixes.md#area-1-implementation-records)

Branch `stage6-area1-propagation` from `e845caa`. Plan commit `dc6bbe9`. Implementation commits through `e47c704`. Each commit verifies with signature status `G`.

## Task results

| Task and commit | Regressions | Golden changes and decisions |
|---|---|---|
| 1. Coordinate primitives, `47d1354` | Four geodesy regressions cover destination, midpoint, antipode and locator alphabet. The corner golden checks all 55 locator and precision cases. | Twelve destination longitudes and one midpoint wrap. The north-east locator starts with RR. The existing precision tolerance means the centre is indistinguishable from the corner at precision 7 and above. Autolength stops at 7. The 6371 km sphere follows the owner's decision. |
| 2. Area coverage, `a096d83` | GUI path box, CLI path and horizon preparation, wrapped missing-tile bounds, closed Maidenhead squares. | No golden changes. The wrapped box uses the existing min-longitude-greater-than-max convention. The default area includes 624 squares. Input endpoints are validated before wrapping generated points. |
| 3. Tile lookup and local obstruction, `f154874` | Reworked constructor, sector and FIFO tests. Added shared-corner and global-edge tests. | The 16 changed samples are in eight ASTER3 golden profiles, whose moving averages consume the corrected corner. The expected values come from direct fixture tile selection and the five-point mean. The FIFO test removes the backing rows to check which tiles were retained. |
| 4. Paths and profiles, `c04a86e` | Added Fresnel, short receiving leg, single-cell ridge and profile endpoint regressions. Updated model, formula, path, CLI and chart tests. | Recomputed path angles and derived heights from the actual legs. Zero-length Fresnel radii become zero. The info-point array retains its length and ends at the receiving station. CLI `dist_km` is a JSON number that can represent a fractional terminal distance, and the GUI chart uses the same distance. Explicit golden step widths still test the original sample grid. |
| 5. Horizons, `c593aed` | Recomputed the expected angles at every golden azimuth and checked the obstruction flag on create and load. | Every horizon angle decreases in the fixture. Terrain height and distance stay at the C# winning obstacle. The local obstruction flag is a per-load comparison and has no golden field. |
| 6. Intersection, `5b6b5b3` | Added exact and near-meridian regressions and the positive-limit test. Replaced the midpoint skips with an independent great-circle reference. | The polar cases use the great-circle normal reference, including when both tracks coincide. Near the meridian the original distance formula suffered cancellation after the bearing was corrected, so the fix uses its factored form there. The 216 limit-only changes are listed below. |
| 7. Horizon bands, `ba76f6c` | Added a synthetic CLI profile where an early band cut hid a later ridge. Compared every golden winner with the uncut maximum and checked the CSV columns against full-profile results. | Seven golden retained lengths grow by one. The 70 MHz synthetic case now sees a 2.5697933° angle instead of −∞. Each band receives a copy because the library's public function still returns its cut via the input vector. |
| 8. Cache versions, `b4d1fab` | Added both legacy-schema migration tests, stale-version replacements and cache tests for 100 m and 500 m. Changed the setting from a progress callback to verify a parallel horizon uses one snapshot. | No golden changes. Each of the three computed tables has its own algorithm version. The migration rebuilds computed tables and preserves source tables. Concurrent openers recheck SQLite schema versions under the write lock before applying migration steps. |
| 9. Integration fixtures, `e47c704` | Calculator lookup keys and CSV rows follow the 500 m GLOBE default. Both near-antipodal web routes return profiles and report missing coverage. | No golden changes. The first workspace runs exposed stale expectations for these specific behaviors. The follow-up updates those fixtures. |

## Finding status

- Fixed #1, #2, #3, #4, #5, #8(a/b), #10, #11, #15, #282, #289, #343, #344, #345, #346, #347, #349, #350, #351, #352 and #354.
- Kept #7, #179, #348 and #353.
- Blocked none.

## Golden case indices for #2

The cases are from `propagation_path.json:intersection_point` at 50 km. Each accepted point uses the unlimited case two entries earlier. These are the changes outside the cases affected by #344. The combined fixes change result presence in 96 of those cases.

**Point to null (180 cases)**

```text
25, 28, 31, 43, 46, 64, 67, 70, 82, 85, 113, 116, 119, 131, 134, 152, 155, 158, 170,
173, 201, 204, 207, 219, 222, 240, 243, 246, 258, 261, 333, 336, 339, 351, 354, 372,
375, 378, 390, 393, 421, 424, 427, 439, 442, 460, 463, 466, 478, 481, 509, 512, 515,
527, 530, 548, 551, 554, 566, 569, 659, 662, 698, 701, 747, 750, 786, 789, 835, 838,
874, 877, 946, 955, 958, 964, 985, 994, 997, 1003, 1034, 1043, 1046, 1052, 1073, 1082,
1085, 1091, 1122, 1131, 1134, 1140, 1161, 1170, 1173, 1179, 1254, 1257, 1263, 1266,
1275, 1278, 1293, 1296, 1302, 1305, 1314, 1317, 1342, 1345, 1351, 1354, 1363, 1366,
1381, 1384, 1390, 1393, 1402, 1405, 1430, 1433, 1439, 1442, 1451, 1454, 1469, 1472,
1478, 1481, 1490, 1493, 1565, 1571, 1574, 1583, 1586, 1604, 1610, 1613, 1622, 1625,
1653, 1659, 1662, 1671, 1674, 1692, 1698, 1701, 1710, 1713, 1741, 1747, 1750, 1759,
1762, 1780, 1786, 1789, 1798, 1801, 1885, 1891, 1894, 1924, 1930, 1933, 1973, 1979,
1982, 2012, 2018, 2021, 2061, 2067, 2070, 2100, 2106, 2109
```

**Null to point (36 cases)**

```text
644, 647, 650, 683, 686, 689, 732, 735, 738, 771, 774, 777, 820, 823, 826, 859, 862,
865, 949, 988, 1037, 1076, 1125, 1164, 1568, 1607, 1656, 1695, 1744, 1783, 1873, 1912,
1961, 2000, 2049, 2088
```

## Gates

All Cargo commands used `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`.

| Command | Result |
|---|---|
| `cargo fmt --all --check` | Pass |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Pass |
| `cargo test --workspace --locked` | Pass, 1,385 passed, 0 failed, 5 ignored |
| `cargo clippy -p as-cat -p airscout -p catcheck --all-targets --locked --features as-cat/hamlib,airscout/hamlib,catcheck/hamlib -- -D warnings` | Pass |
| `cargo clippy -p as-cat --all-targets --locked -- -D warnings` | Pass |

Workspace inventory increased from 1,371 to 1,390 tests, including five ignored tests. The baseline is the final inventory minus 19 added test functions. Golden JSON and Cargo.lock match `e845caa`. Docker gates are assigned to the controller.

## Final review fix wave

Final: fixed #344 polar crossing outside the meridian cutoff — `finding_344_polar_crossing_across_meridian_cutoff` RED→GREEN.

Final: fixed #10/#15/#289 closed antimeridian coverage and sampling — `finding_289_closed_antimeridian_edge_uses_the_sampled_square` RED→GREEN.

Final: fixed #351 ASTER ridge guarantee — `finding_351_aster_half_step_preserves_and_adds_filtered_ridges` RED→GREEN.

Final: fixed golden delta audit and unchanged C# flags — `propagation_path_matches_original` RED→GREEN. The [per-case manifest](../../logic-fixes.md#per-case-golden-deltas) also covers the elevation, horizon and geodesy fixtures.

### Deferred minor findings

- Path coverage checks only the endpoints before treating missing tiles as sea. A path may leave and reenter the prepared rectangle between endpoints. This also applies to ASTER filter padding.
- The #10 elevation golden's zip comparison can accept a vector shorter than its separately checked count.

### Final review gates

All five host gates above passed for the final fix wave. The workspace suite finished with
1,388 passed, 0 failed, and 5 ignored. The three computed cache algorithm versions are now
3. The C# JSON fixtures and Cargo.lock match `e845caa`. Docker gates are assigned to the
controller.

## Controller close-out (2026-11-25)

- Final review (Codex, independent): 4 Important (#344 near-meridian crossing, +180° coverage vs. tile lookup, #351 ASTER halving lowering the horizon, golden records / C# flag assertions); one fix wave e6704ac..0cf3b41; scoped re-review: all 4 ADDRESSED, no new Critical/Important.
- Deferred minors: coverage check uses endpoints only (path can leave and re-enter the prepared area); elevation golden zip comparison can accept a short sample vector when the count matches.
- Gates at 0cf3b41: macOS fmt, clippy, workspace tests (1,388 passed, 0 failed, 5 ignored), hamlib pair; Linux docker LINUX_OK; Windows mingw WINGNU_OK.
