# Area 4 golden deltas

Golden exception: the `elevation_tilegen.json` `srtm_tile` group no longer asserts C# equality — the #294 one-row-south shift changes 863/864 cases and their values are opaque FNV hashes — owner-approved 2026-11-26 (see `docs/logic-fixes.md#area-4-implementation-records`); no delta manifest was generated and `tests/golden/*.json` is unchanged.
