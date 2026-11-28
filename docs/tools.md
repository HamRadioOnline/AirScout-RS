# Command line and tools

Use `--help` on any tool or subcommand below for its options.

**`airscout-cli`** runs propagation and aircraft queries and prints JSON.
Positions are Maidenhead locators or `lat,lon`.

```sh
airscout-cli path --from JO50AA --to JO61UF --band 144 --data ./data --download
airscout-cli feeds
```

**`airscout-cli wintest-sim`** sends a Win-Test message and can listen for replies.

```sh
airscout-cli wintest-sim --msg ASSETPATH --data "1443000,TE1ST,JO50AA,TE2ND,JO61AA" --listen 5
```

**`airscout-cli horizon-gen`** writes a horizon CSV per station using local elevation data.

```sh
airscout-cli horizon-gen --data ./data --stations all --out ./horizons
```

**`catcheck`** tests CAT using the app's settings or command-line overrides.

```sh
catcheck --engine rigctld --host localhost --port 4532 status
```

Interrupting its Doppler test can leave the rig changed. Use the values printed
on the `saved:` line to restore it by hand.

**`elevation-tile-gen`** makes elevation tiles, square ZIPs and catalogues from
local SRTM or GLOBE data. The source data licences apply.

```sh
elevation-tile-gen tiles ./srtm3/*.hgt --out ./www/ElevationData/SRTM3
```

**`airscout-cli elevation-coverage`** compares a local or downloaded catalogue
with the local elevation store.

```sh
airscout-cli elevation-coverage --download --model srtm3 --data ./data --geojson coverage.json
```

**`maptile-gen`** imports and exports local map tiles and combines PNGs. Point
`--db` at the app's tile store to use the imported tiles in AirScout.

```sh
maptile-gen import --db maptiles.gmdb ~/old/Data.gmdb
```

## Golden tests

Tests use the C# reference results in `tests/golden/` and need no internet
access or .NET. Regenerate the fixtures with `tools/golden-harness/run.sh`
(Docker). See [CONTRIBUTING.md](../CONTRIBUTING.md) for the CI checks.
