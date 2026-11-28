# Golden fixtures

JSON files generated from the **original AirScout C# code** (dl2alf/AirScout, commit
`91b02cb7f4210f3cf1e2cb7c3666ce29b65e69c5`) by `tools/golden-harness`. Rust tests compare
against them, so the port reproduces the original behaviour exactly (including its bugs,
see `docs/logic-findings.md`).

Format: `{"upstream_commit": "<sha>", "cases": [{"fn": "<name>", "args": {...}, "result": ...}]}`

- Doubles use round-trip formatting. NaN/±Infinity are the strings `"NaN"`, `"Infinity"`, `"-Infinity"`.
- A C# `null` result is JSON `null`.
- A thrown exception is `{"error": "<ExceptionTypeName>"}`.
- `elevation_tilegen.json` comes from synthetic DEM sources that the harness builds in a temp dir and deletes; the `source` cases carry their rectangles, and the file holds only digests and numbers.

Regenerate (needs either a local .NET 8 SDK or docker):

```sh
tools/golden-harness/run.sh
```

Do not edit these files by hand.
