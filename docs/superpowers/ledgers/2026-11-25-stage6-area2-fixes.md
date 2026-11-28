# Stage 6 area 2 implementation ledger

[Implementation plan](../plans/2026-11-25-stage6-area2-fixes.md) · [Design](../specs/2026-11-25-stage6-logic-fixes-design.md) · [Finding-by-finding records](../../logic-fixes.md#area-2-intersection-and-aircraft-prediction)

Branch `stage6-area2-aircraft` from `800c227`.

## Task commits

| Task | Commit | Signature |
|---|---|---|
| 1. Desktop forecast source | `c1573d5` | G |
| 2. Replay clock | `25674f2` | G |
| 3. Aircraft validation and lookup | `cd2eb01` | G |
| 4. Retention | `da09748` | G |
| 5. Motion and analysis snapshots | `05e1317` | G |
| 6. Crossing geometry and consumers | `613e552` | G |
| 7. History and Doppler | `f5015a2` | G |
| 8. ADS-B words and framing | `a1a82fa` | G |
| 9. Airborne pairing and emission | `9d1bb24` | G |
| 10. Live admission | `2775b2c` | G |

## Task results

| Task and commit | Regressions | Golden changes and decisions |
|---|---|---|
| 1. Desktop forecast source (#370), `fix(airscout): stage 6 #370 serve current aircraft forecasts` | RED on pre-fix `HEAD` with the new regression (`finding_370_legacy_forecast_is_ignored_after_restart`) in a detached temporary worktree; GREEN after the implementation with `finding_370_legacy_forecast_is_ignored_after_restart` and `finding_370_forecasts_at_request_time_and_expires`. | No golden fixture change. `/planes.json` now ignores legacy persisted forecasts, projects from live cache at request UTC with configured TTL, and returns valid empty JSON when no report is eligible. |
| 2. Replay clock (#109), `fix(airscout): stage 6 #109 use replay instant for nearest prediction` | RED before the fix with `history_uses_a_single_analysis_instant_across_paths` (`[100,0]` when a second path consumed a wall-clock-shifted mutation) and GREEN after switching to a single analysis instant plus the explicit HISTORY replay regression `finding_109_history_tick_uses_replay_instant_for_all_paths`. Verified with `cargo test -p airscout --test history_mode --locked` and `cargo test -p airscout --test pipeline --locked`. | No golden fixture change. HISTORY now evaluates replay snapshots and nearest crossings at one shared replay instant; path order is no longer affected by a separate wall clock in this stage-6 row. |
| 3. Aircraft validation and lookup (#16, #18, #19, #21, #367), `fix(as-aircraft): validate measurements and identifiers` | RED with `finding_16_units_use_si_factors`, `tooltip_imperial`, `finding_16_web_unit_converter_uses_correct_mile_factors`, `finding_18_aircraft_type_lookup_prefers_populated_iata`, `finding_19_call_and_registration_validation_share_discriminator`, `finding_21_route_split_uses_full_airport_codes` and `finding_367_plane_rejects_nan_required_measurements`; GREEN after fixing units, type-row ranking, shared call/registration discriminators, route splitting and finite plane checks. | Added documented checker deltas in `docs/logic-fixes-area2/golden-deltas.tsv`: `plane_checker.json` `check_call[281,301,321]` false→true, `to_call[292,312,332]` null→normalized input, `check_reg[702]` true→false, `to_route[1071,1171]` null→input, `to_route[1111]` input→null and `check_plane[2123]` true→false. No immutable C# JSON file changes. |
| 4. Retention (#30, #31), `fix(as-aircraft): enforce configured position history retention` | RED before the fix with `finding_30_maintain_trims_to_count_with_stable_ties` (count remained 2,005 over limit after one pass) and `finding_31_size_limit_uses_current_occupied_pages_each_pass` (size trimming stayed at 0 with a receded historical peak). GREEN after processing expiry first, count trimming to the configured ceiling in bounded oldest-first batches with stable timestamp ties, and evaluating size each pass from occupied pages. Verified with `cargo test -p as-aircraft --test positions --locked`. | No golden fixture change. Retention now keeps exactly the newest configured count in one maintenance pass and continues trimming while occupied pages exceed the configured limit, independent of historical file-size peaks. |
| 5. Motion and analysis snapshots (#23, #355, #356, #24), `fix(as-aircraft): align and transport aircraft forecasts` | RED before the fix with `finding_23_track_wrap_uses_shortest_turn_and_normalizes`, `finding_355_speed_ramp_uses_interval_average`, `finding_356_projection_transports_tangent`, and `finding_24_crossing_uses_analysis_snapshot_without_mutating_inputs`; GREEN after shortest-turn track extrapolation, speed-ramp distance integration, spherical tangent transport, and immutable crossing-time snapshots with future-report rejection. Verified with `cargo test -p as-aircraft --test cache --locked`, `cargo test -p as-aircraft --test cache_golden --locked`, and `cargo test -p as-aircraft --test crossing_golden --locked`. | Added documented deltas for `plane_cache.json` and `nearest_planes.json` in `docs/logic-fixes-area2/golden-deltas.tsv` (Task-5 rows). `plane_cache` updates include wrapped heading progression, ramp-distance movement, and transported local course; `nearest_planes` updates include analysis-time snapshot timestamps/positions and unchanged `caller_after` inputs across passes and path order. |
| 6. Crossing geometry and consumers (#357, #358, #368, #82, #25, #26, #27), `fix(as-aircraft): separate proximity from crossing ETA` | RED before the fix with `nearest_plane_picks_potential_then_qrb`, `finding_25_near_branch_uses_negative_tolerance`, `finding_26_approaching_sets_theta_from_forward_crossing`, `finding_82_global_nearest_survives_empty_later_path`, `finding_358_alarm_uses_strict_forward_distance_cutoff`, `finding_27_common_floor_and_zero_disabled_angle_limits`, `nearest_reports_null_eta_for_near_receding_plane` and `nearest_data_lists_crossing_planes_only`; GREEN after finite-arc proximity geometry, forward-ETA separation, inclusive nearest cutoffs, global nearest retention across paths, strict alarm forward cutoff, and shared GUI candidate filters with zero-disabled per-band angle limits. Verified with `cargo test -p as-aircraft --test crossing_golden --locked`, `cargo test -p airscout --test pipeline --locked`, `cargo test -p as-cli --test cli --locked`, `cargo test -p airscout --test crossing_history --locked`, `cargo test -p airscout --test style --locked`, `cargo test -p airscout --test ui_planes --locked`, `cargo test -p airscout --test wintest_nearest --locked` and `cargo test -p airscout --test web_paths --locked`. | Updated `nearest_planes.json` delta rows in `docs/logic-fixes-area2/golden-deltas.tsv` for cases `[0–9]` with independent references and findings tag `#25/#26/#357/#358/#368/#82`. No immutable C# fixture changes. CLI now emits `forward_distance_km` and nullable `minutes`, and Win-Test omits rows without forward ETA. |
| 7. History and Doppler (#127, #128, #369, #133, #136, #189), `fix(airscout): evaluate crossing history at each replay step` | RED with `finding_136_marks_groups_with_multiple_hexes_only`, `finding_127_history_windows_keep_source_data_immutable`, `finding_128_history_steps_keep_running_after_gaps`, `finding_128_preroll_and_exact_step_supersession`, `finding_369_history_uses_configured_radius_not_half_path_default`, `finding_369_entry_by_motion_is_evaluated_at_each_step`, `finding_133_history_path_matches_live_path_resolver`, `finding_189_pause_and_gaps_reset_doppler_baseline` and `finding_189_geometry_change_resets_doppler_baseline`; GREEN after replacing aliasing history windows with owned sliding windows, per-step replay evaluation with TTL pre-roll, shared live/history path resolution, strict ambiguity grouping and Doppler baseline invalidation on discontinuities. Verified with `cargo test -p airscout --test crossing_history --test crossing_csv --test tracker --locked`. | No golden fixture change. History and Doppler fixes are covered by synthetic regressions; immutable C# JSON fixtures remain unchanged. |
| 8. ADS-B words and framing (#33, #34, #36, #40, #360, #361, #362), `fix(as-adsb): decode framing and position fields correctly` | RED with compile/test failures from the new regressions (`finding_40_beast_unstuffs_escaped_markers_across_chunks`, `finding_360_local_cpr_wraps_antimeridian_and_rejects_past_pole`, `finding_361_surface_global_requires_reference`) and golden-delta assertions in `adsb_golden`/`decoder_golden`; GREEN after signed velocity components, subtype-4 airspeed scaling with decoder gating, airborne latitude-range rejection, Beast unstuffing across chunk splits, local CPR canonicalization/pole rejection, trusted-reference surface global CPR, and Gillham/Q altitude fixes. Verified with `cargo test -p as-adsb --locked`. | Added documented deltas in `docs/logic-fixes-area2/golden-deltas.tsv` for `adsb_msgs` and `adsb_decoder` (#33/#34/#36/#361/#362). No immutable C# fixture file changes. Framing and local/surface CPR reference behavior are covered by Rust regressions outside immutable JSON fixtures. |
| 9. Airborne pairing and emission (#42, #359, #43, #44, #45, #35, #363, #366, #364), `fix(as-adsb): qualify paired fixes and height observations` | RED with new regressions in `crates/as-adsb/tests/task9_regressions.rs` (`finding_42_pair_window_accepts_10s_and_rejects_10s_plus_1ns`, `finding_43_expiry_removes_stale_incomplete_aircraft_independently`, `finding_45_delayed_identity_emits_once_when_position_already_known`, `finding_35_raw_zero_unavailable_and_raw_one_zero_valid`, `finding_363_tc20_decodes_direct_geometric_height`, `finding_366_ttl_is_inclusive_and_pressure_is_marked_estimate`) and with golden-delta assertions in `adsb_golden`/`decoder_golden`; GREEN after receipt-time pairing with strict 10 s bound, parity persistence through local decode, TTL expiry, finite coordinate gating, position-revision emission, typed altitude slots (pressure estimate vs direct geometric), and per-field freshness checks for speed/heading/altitude components. Verified with `cargo test -p as-adsb --locked`, `cargo test -p as-feeds --test rtl1090 --test pipeline --locked`, `cargo test -p airscout --test crossing_history --test pipeline --locked`, and `cargo test -p as-aircraft --test crossing_golden --locked`. | Added documented deltas for `adsb_msgs` and `adsb_decoder` in `docs/logic-fixes-area2/golden-deltas.tsv` (#35/#42/#45/#359/#363). No immutable C# fixture changes. Pressure and pressure-plus-correction heights now carry explicit estimate provenance while geometric stays explicit non-estimate. |
| 10. Live admission (#365), `fix(as-feeds): admit reports against receipt time` | RED on the pre-fix worker with `worker_holds_future_reports_out_of_every_sink` (the future report was published in `FeedEvent::Planes`); GREEN after receipt-time quarantine, pending promotion on successful/empty/error cycles, and stale-pending suppression against the eligible cache. Boundary coverage checks receipt, +1 ns, +10 s and +10 s +1 ns, with promotion only once each timestamp is reached. `cargo test -p as-feeds --locked`: 99 passed, 2 ignored; `cargo test -p as-aircraft --locked`: 66 passed, 1 ignored. `cargo fmt --all --check` and `cargo clippy -p as-feeds -p as-aircraft --all-targets --locked -- -D warnings` passed. | No golden fixture changes. Future reports are withheld from cache, metadata, position history and plane events; pending reports preserve event time, do not displace a newer eligible report, and replay cache reads remain past-only. |

## Finding status

- Fixed #16, #18, #19, #21, #23, #24, #25, #26, #27, #30, #31, #33, #34, #35, #36, #40, #42, #43, #44, #45, #82, #109, #127, #128, #133, #136, #189, #355, #356, #357, #358, #359, #360, #361, #362, #363, #364, #365, #366, #367, #368, #369 and #370.
- Kept #17, #20, #22, #28, #29, #32, #37, #38, #39, #131, #132 and #186.
- Blocked none.

## Row candidates

- #44 has a `decoder_golden` implementation record, but no focused synthetic regression currently rejects non-finite or out-of-domain coordinates at emission. Add one if this decoder contract is revisited.

## Task 11 audit and gates

- Verified the seven documentation minors already present in `800c227`; no repeat changes were made for them.
- Added missing area-2 implementation records for #35, #42, #43, #44, #45, #359, #363, #364 and #366. Added **Kept** labels and area-2 pointers for all 12 approved keep rows.
- Audit result: all 43 area-2 `fix` rows have **Fixed in stage 6** labels, pointers and implementation records; all 12 `keep` rows have **Kept** labels. The implementation records cite upstream C# locations at `fbadef0f`, the equivalent C# change and existing Rust test names.
- The 144 rows in `docs/logic-fixes-area2/golden-deltas.tsv` are in five consumed groups (`plane_checker`, `plane_cache`, `nearest_planes`, `adsb_msgs`, `adsb_decoder`). Each group is loaded by its golden test, which calls `DeltaManifest::finish()` to reject unexercised rows.
- `git diff --stat 6ef3079..HEAD -- tests/golden Cargo.lock`: empty.
- The first workspace test runs exposed a localhost test race: `beast_stream_yields_plane` (and once `silent_receiver_reconnects`) stopped the feed before the sender completed its writes, causing `BrokenPipe`. Both tests now join the finite sender before stopping the feed; the RTL1090 integration target and final workspace suite pass.

| Gate | Exact command | Result |
|---|---|---|
| Format | `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo fmt --all --check` | Pass |
| Workspace Clippy | `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo clippy --workspace --all-targets --locked -- -D warnings` | Pass |
| Workspace tests | `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test --workspace --locked` | Pass; 1,422 passed, 0 failed, 5 ignored across 200 test targets |
| Hamlib Clippy | `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo clippy -p as-cat -p airscout -p catcheck --all-targets --locked --features as-cat/hamlib,airscout/hamlib,catcheck/hamlib -- -D warnings` | Pass |
| Featureless as-cat Clippy | `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo clippy -p as-cat --all-targets --locked -- -D warnings` | Pass |

## Notes

- The desktop writer loop and persisted `<data>/tmp/planes.json` response path were retired. `/AircraftList.json` continues reading `vrsplanes.json` from `<data>/tmp`.
- The FR24 output schema stays unchanged (`"version":1` for `/planes.json`).
- Position retention now trims by deterministic oldest-first order (`LastUpdated`, then `rowid`) and uses occupied pages (`page_count - freelist_count`) as the size metric.

## Final review

- #359 local CPR envelope: RED before the fix, GREEN with `finding_359_rejects_local_alias_whose_envelope_touches_pole`, `finding_359_accepts_midlatitude_local_fix_at_ten_seconds`, and `finding_359_rejects_candidate_beyond_cap_and_cpr_quantum`. The fallback now enforces the inclusive 10 s reference age, 6,000 kt displacement cap, both fixes' CPR quantization, a single NL band, strict parity-cell half-widths, and pole-safe latitude/longitude envelopes. No golden fixtures changed.
- #364 altitude provenance: RED before the fix (missing provenance API), GREEN with RTL1090 pressure-to-PlaneInfo, VRS geometric-source, crossing-copy, legacy-history-Unknown, and GUI/HTTP output regressions. Position history remains schema-compatible; no immutable fixtures changed.
- Linux gate fixes: `as-golden` delta comparison now applies the tolerance inside objects and arrays (`equivalent_applies_tolerance_inside_objects_and_arrays`, RED→GREEN); two `plane_cache` delta rows (cases 0 and 1) were pure float noise and were removed. `crossing_golden` tolerance raised to 1e-6 (cross-target libm noise up to ~4e-9 m on `alt_diff`; no `nearest_planes` delta row lies within 1e-6 of its C# value).
- #364 follow-up: OpenSky `/api/states/all` index 13 is `geo_altitude`, so its provenance is Geometric (test in `opensky_maps_fixture`, RED→GREEN). GPS altitude in ADSB support stays Unknown (no stated datum).
- A superseded Codex run resumed after its usage limit and committed `0930ae3` (#42/#43 event vs receipt clocks, #44 focused emission regression `finding_44_emits_only_finite_coordinates_in_domain`) and `974aeb9` (#366 component deadline into prediction). It was cancelled; both commits were kept after macOS gates and a scoped review. Its uncommitted `cache.rs` change was set aside, not applied.
- Scoped review of `0930ae3..974aeb9` (Important): the pipeline borrowed cached speed or height for a component-timed report while keeping the fresh component deadline. Component-timed reports now reject instead (`finding_366_component_reports_do_not_borrow_cached_motion_or_height`, RED→GREEN).
- Ruling: the #44 minor from the final review is closed by `0930ae3`.
- Linux follow-up: `avr_stream_yields_plane` had the same localhost BrokenPipe race as the Beast tests; the finite sender now finishes before the feed stops (`89999f5`).

## Close-out

| Gate | Tree | Result |
|---|---|---|
| macOS fmt, workspace Clippy, workspace tests | `c74d10f` | Pass; 1,438 passed, 0 failed |
| macOS hamlib Clippy pair | `974aeb9` (no `as-cat`/`airscout` change after) | Pass |
| Linux Docker (`clippy`, `test --no-fail-fast`, hamlib pair) | `89999f5` | LINUX_OK; 1,438 passed, 0 failed |
| Windows mingw | `c74d10f` (only a test file changed after) | WINGNU_OK |

- Final review (Copilot gpt-6-luna; Codex was at its limit): 2 Important (#359, #364), 1 Minor (#44). Both Important fixed and re-reviewed clean; #44 closed by `0930ae3`. A scoped review of the resumed Codex commits found 1 Important, fixed in `c74d10f`.
- `git diff --stat 6ef3079..HEAD -- tests/golden Cargo.lock`: empty.
- Deferred minors: none open for area 2.
- Merged to `main` by fast-forward and pushed (owner authorization for per-area merges, 2026-11-25).
