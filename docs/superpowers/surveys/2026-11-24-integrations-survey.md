# AirScout integrations survey (stage 4: Win-Test, CAT, web server, DDE, Track outputs)

Base directory for all refs: `/Users/philipp/Projects/hamradioomline/AirScout-RS/target/upstream/AirScout/`.
Refs use the form `Dir/File.cs:line`. `AirScout/` is the WinForms project (so `AirScout/MapDlg.cs` is the main window).
"Settings" means `AirScout/Properties/Settings.settings` unless another file is named.
Stage 3 (GUI) already ports the tracking maths and the display; this survey covers the outputs and the network and hardware inputs.
Out of scope (one-line notes only): stage 5 tools. `WinTestSimulator` uses the `WinTest/` project unchanged (`WinTestSimulator/MainDlg.cs:20-41`, sends one message to the broadcast address). `CATCheck` links `ScoutBase/ScoutBase.CAT` (`CATCheck/CATCheck.csproj:555`). `AirScoutPlaneServer/Webserver.cs` is a separate server and not part of this survey.

Summary of the shape of stage 4:
- Four independent integrations plus one worker that fans out tracking values.
- Everything runs on `BackgroundWorker` threads that are started and stopped in bulk (section 1).
- Nothing in the C# code authenticates anything. Two sockets listen on all interfaces.
- HamLibSharp is in the repository but no project references it (section 4.6). Native hamlib support does not exist upstream.

Rough C# line counts:
- WinTest wire code: `WinTest/WinTest.cs` 155, plus the receive/dispatch region `AirScout/MapDlg.cs:7001-7625` (about 625). `AirScout/WinTest.cs` (136) is a stale copy, see 2.7.
- Web server: `AirScout/Webserver.cs` 1253, `AirScout/WebserverStartArgs.cs` 15, `JSONWriter` region `AirScout/MapDlg.cs:8460-8545` (85). wwwroot: 1384 files, 29 MB, of which the AirScout-specific part is 10 JS files (4302 lines), 1 CSS (133 lines) and about 48 PNG icons. The rest is a CoreUI admin template.
- Track outputs: `AirScout/MapDlg.cs:7836-8450` (about 615), `AirScout/TrackValues.cs` 25.
- CAT: `AirScout.CAT/` 3374 (`OmniRigX.cs` 1389, `ScoutBaseRig.cs` 706, `ExtOmniRig.cs` 526 unused, `CATWorker.cs` 325), `ScoutBase/ScoutBase.CAT/` 4577, `HamLibSharp/` 10846 (unreferenced).
- DDE: `NDDE/` 14990 (a whole library); AirScout uses only `DdeClient.TryConnect`, `TryPoke`, `IsConnected`, `Disconnect` (3 call sites).
- Options tabs involved: Network `AirScout/OptionsDlg.cs:2583-2600`, SpecLab `:2602-2617`, Track `:2617-2742`, CAT `:2744-3080`.

---

## 1. Lifecycle and threading (how everything is started, stopped and restarted)

Workers are `BackgroundWorker` instances created in the designer (`AirScout/MapDlg.Designer.cs:141-174`, `:1589-1999`), except `bw_CAT` and `bw_CATUpdater` which are constructed inline (`AirScout/MapDlg.cs:494-495`).

Start (`StartAllBackgroundWorkers`, `AirScout/MapDlg.cs:2402-2457`):
- `Server_Activate` true: starts `bw_WinTestReceive` and `bw_Webserver` together (`:2404-2414`). One setting gates both.
- `SpecLab_Enabled`: `bw_SpecLab_Receive` (`:2416-2420`).
- `Track_Activate`: `bw_Track` (`:2422-2426`).
- `AirScout.CAT.Properties.Settings.Default.CAT_Activate`: `bw_CATUpdater` and `bw_CAT` (`:2428-2453`). `bw_CAT` gets a `CATWorkerStartOptions` snapshot of the CAT settings (`:2439-2452`).
- `bw_JSONWriter` is started once at startup (`:2173-2174`), not by the Options cycle.

Stop (`StopAllBackgroundWorkers`, `AirScout/MapDlg.cs:2477-2503`): cancels each worker and busy-waits with `Application.DoEvents()` until `IsBusy` is false (`:2469-2472`). The `timeout` variable at `:2465` is never used, so the wait has no limit.
The stop list contains `bw_WinTestReceive`, `bw_SpecLab_Receive`, `bw_Track`, `bw_JSONWriter`, `bw_CATUpdater` and `bw_CAT`. **`bw_Webserver` is not in the list** (it is only cancelled in `CancelAllBackgroundWorkers` at `:2524`, which runs on shutdown).

How a settings change restarts the integrations:
- Options is opened from `ShowOptionsDlg` (`AirScout/MapDlg.cs:3882-3992`). It calls `StopAllBackgroundWorkers`, saves settings, shows the dialog, then calls `StartAllBackgroundWorkers` again (`:3893`, `:3976`). So WinTest, Track, SpecLab and CAT pick up new values by being restarted.
- The web server does not restart. `StartAllBackgroundWorkers` skips it while `bw_Webserver.IsBusy` (`:2412`), and `GetContext` blocks (`Webserver.cs:141`), so a changed `Webserver_Port` or unticked `Server_Activate` only takes effect after an application restart. The `bw_Webserver` listener keeps the old port until then.
- Within a worker, settings are read live from `Properties.Settings.Default` on every iteration (for example `Track_Activate` in the inner loop `:8054`, `Doppler_Strategy_*` at `:8252-8277`). They are not snapshotted, except CAT (`CATWorkerStartOptions`).
- `bw_Track` results go to the UI thread through `ReportProgress` codes: -1/0 text, 1 `TRACKSTATUS`, 2 `ROTSTATUS`, 3 `TrackValues` (`:8352-8450`). The UI thread then writes `bw_CAT.DopplerStrategy`, `RxFrequency`, `TxFrequency` (`:8383-8449`). Those are plain fields read by the CAT thread without any lock.

Rust notes (suggestions only): the spec's tokio worker model fits. Give each integration a `start(settings_snapshot)` / `stop().await` pair and let the Options dialog call stop-all and start-all, as upstream does. The web server should take part in that cycle (decision D3 in section 10).

---

## 2. Win-Test UDP protocol (`as-wintest`)

### 2.1 Public surface the GUI calls
- `WinTest/WinTest.cs`: enum `WTMESSAGES` (`:8-24`) and class `wtMessage` (`:26-153`) with constructors from bytes (`:36-45`) and from fields (`:47-67`), `FromBytes` (`:69-103`), `ToBytes` (`:105-148`). The class `WinTest` at `:150-152` is empty. `WinTest/SupportFunctions.cs` has one unused enum-description helper.
- Receiver: `bw_WinTestReceive_DoWork` (`AirScout/MapDlg.cs:7005-7069`), dispatcher `DispatchWinTestMsg` (`:7566-7599`), main-thread handler `bw_WinTestReceive_ProgressChanged` (`:7601-7618`).
- Senders: reply `ASNEAREST` inside `ASSetPath` (`:7268-7301`), Track outputs `Track_UDP_WinTest` / `Track_UDP_AirScout` (`:7864-7895`).

### 2.2 Ports, addresses, sockets
- Receive: UDP socket bound to `0.0.0.0:Server_Port` (default 9872, `:7030-7041`). `SO_REUSEADDR` set, `ReceiveTimeout` 1000 ms so the loop can see cancellation. No `EnableBroadcast` is needed for receive. A bind failure is only logged (`:7038-7041`), see bug L1.
- Reply to `ASSETPATH`: new `UdpClient`, `SO_REUSEADDR`, `Connect(255.255.255.255:Server_Port)`, one `Send`, `Close` (`:7295-7301`). Same port as the listener, so AirScout also receives its own reply; it is dropped by the destination filter below.
- Track output: `UdpClient` to `255.255.255.255:Track_UDP_WinTest_Port` (default 9871) or `Track_UDP_AirScout_Port` (default 9872), created once per worker pass (`:8024-8033`). Only one of the two is active (`if / else if`).
- Neither send path sets `EnableBroadcast`. On .NET Framework on Windows this works; on Mono/.NET Core it is an access error on most OSes. A Rust port must call `set_broadcast(true)` (suggestion).
- Filtering of received messages: only messages whose `Dst` equals `Server_Name` (default `"AS"`, case-sensitive) are dispatched (`:7054-7056`). `Src` is never checked.
- Authentication: none. The checksum is computed on receive (`WinTest/WinTest.cs:93-102`) but `HasChecksum` is never read by the receiver. Any host on the LAN can send `ASSHOWPATH` or `ASADDWATCH`.

### 2.3 Wire format (byte-exact)
Text, 7-bit ASCII, one datagram per message, no newline. Layout built in `wtMessage.ToBytes`:

    <NAME>: "<Src>" "<Dst>" "<Data>"<Extra><pad><CS>

- `<NAME>` is the enum member name as text (`Msg` concatenated, `WinTest/WinTest.cs:121`, `:135`), for example `ASNEAREST`. The numeric enum value is never sent.
- Separator after the name: colon plus one space (`": "`). Fields `Src`, `Dst`, `Data` are each wrapped in double quotes and separated by one space.
- `<Extra>` follows the closing quote of `Data` directly. Track messages use it as `" 00 123"` (leading space, see 2.5). Empty for all other messages.
- `<pad>` and `<CS>`: the string is built with a placeholder at the end, converted with `Encoding.ASCII.GetBytes`, and the last byte is overwritten with the checksum.
  - Default branch (all messages except `ASNEAREST`): `"??"` is appended, so the datagram ends with one `?` followed by the checksum byte (`WinTest/WinTest.cs:130-143`).
  - `ASNEAREST` branch: one `?` is appended, so the checksum byte directly follows the closing text (`:115-127`). This emulates the old Win-Test DLL used by wtKST (comment at `:117`).
- Checksum: sum of all bytes except the last, modulo 256 (`byte` wraps), then OR 0x80 (`:123-126`, `:138-141`). The result is always a byte in 0x80..0xFF, so the datagram is not valid ASCII or UTF-8.
- Names that `ToBytes` does not handle: `NONE` and `UNKNOWN` return `null` (`:113-114`, `:128-129`).

Parsing (`FromBytes`, `WinTest/WinTest.cs:69-103`):
1. Decode all bytes with `Encoding.ASCII`. Bytes >= 0x80 become `?`, which is why the checksum byte appears as `?`.
2. Name is the text before the first `": "`. `Enum.Parse` (case-sensitive, also accepts numeric strings); failure gives `UNKNOWN` (`:73-80`).
3. `Src` is up to the next space, `Dst` up to the next space, quotes removed (`:81-84`).
4. Remainder: drop the last character, remove all `"` characters, `TrimEnd('?')` (`:86-90`). The result is `Data`. `Extra` is not parsed back out; it stays in `Data`.
5. `Checksum` field = last raw byte OR 0x80. `HasChecksum` = (that equals the sum of the preceding bytes OR 0x80) (`:92-102`).
- No bounds checks. A datagram without `": "` or without enough spaces throws `ArgumentOutOfRangeException`. The receiver catches it (`MapDlg.cs:7061-7064`) and logs an error line for every stray packet.
- Quotes inside `Data` are removed on receive, so `Data` cannot contain `"`. Callsigns or fields with non-ASCII characters turn into `?` on both directions.

Message names and numeric values in the compiled enum (`WinTest/WinTest.cs:8-24`): `NONE` 0, `STATUS` 1, `PKTRCVD` 2, `SETAZIMUTH` 100, `SETELEVATION` 101, `SETAZEL` 102, `ASADDWATCH` 247, `ASREMOVEWATCH` 248, `ASWATCHLIST` 249, `ASSETPATH` 252, `ASSHOWPATH` 253, `ASNEAREST` 254, `UNKNOWN` 255. `STATUS`, `PKTRCVD` and the numeric values are never used on the wire or in logic.

### 2.4 Messages AirScout receives (all need `Dst == Server_Name`)
`Data` is a comma-separated string. Parsing is in the handlers named below.

- `ASSHOWPATH` (`MapDlg.cs:7071-7187`), handled on the UI thread. Data: `qrg,mycall,myloc,dxcall,dxloc`. Quotes are stripped again per field, locators are cut to the first 6 characters with `Substring(0, 6)` (`:7086`, `:7090`).
  - Effect: pause, select the Single tab, `PathMode = SINGLE`, validate calls and locators, write `MyCall/MyLat/MyLon/MyHeight=10/DXCall/DXLat/DXLon/DXHeight=10` into the settings, look up or create the two locations (`UpdateLocation(..., GEOSOURCE.FROMLOC)`), set the band, `UpdateStatus()`, `Play()`, force the window to the foreground (user32 calls at `:7163`, Windows only), and set `Track_SetAz` to the bearing and `Track_SetEl` to 0 (`:7173-7180`).
  - Frequency to band: exact string match on the frequency in 100 Hz units: `500000` (6 m), `700000` (4 m), `1440000`, `4320000`, `12960000`, `23200000`, `34000000`, `57600000`, `103680000`, `240480000`, `470880000`, `760320000` (`:7132-7155`). Any other value leaves the band unchanged.
- `ASSETPATH` (`:7189-7310`), handled on the receive thread. Data: `qrg,mycall,myloc,dxcall,dxloc` (6-char locators, `Substring(0, 6)` at `:7209`, `:7211`). The band is `(BAND)(qrg / 10000)` (`:7196-7206`), for example 12960000 gives 1296 = `B1_2G`. Computes the path and the nearest planes and answers with `ASNEAREST` (below). Invalid call or locator: silent return, no reply (`:7213-7220`).
- `ASWATCHLIST` (`:7312-7381`), UI thread. Only if `Watchlist_SyncWithKST`. Data: `band,call1,loc1,call2,loc2,...`; the first field is ignored. Replaces the unchecked part of the watchlist (marks unchecked items for removal, re-adds listed ones, removes the rest).
- `ASADDWATCH` (`:7383-7489`), UI thread. Ignored if `Watchlist_SyncWithKST`. Data: `band,call,loc,checked,call,loc,checked,...` (triples; `(a.Length-1) % 3 != 0` rejects the message). An empty locator is filled from the station database. `checked` is `"1"` for a checked entry. Starts play when at least one entry is checked and play mode is PAUSE.
- `ASREMOVEWATCH` (`:7491-7564`), UI thread. Ignored if `Watchlist_SyncWithKST`. Data without a comma removes all entries (also checked ones). Otherwise `band,call,call,...` removes every entry with a matching call (all locators). Pauses when no checked entry is left and mode is FORWARD.

### 2.5 Messages AirScout sends
- `ASNEAREST` (reply to `ASSETPATH`, `MapDlg.cs:7268-7293`): `Src = Server_Name`, `Dst =` the `Src` of the request, `Extra = ""`, ASNEAREST branch of `ToBytes`.
  `Data` = `DateTime.UtcNow.ToString("u")` (`yyyy-MM-dd HH:mm:ssZ`) + `,mycall,myloc,dxcall,dxloc,` + `count` + `,` + for each plane `call,catshort,qrb_km,potential,minutes,`; the whole string is trimmed of trailing commas (`:7289-7293`). With no planes: `...,dxloc,0`.
  Per plane: `Call`, `PlaneCategories.GetShortStringValue(Category)`, `(int)IntQRB`, `Potential`, `mins = (int)(IntQRB / Speed / 1.852 * 60)` (0 if speed <= 0). Only planes with `IntPoint != null && Potential > 0` are listed (`:7274-7284`).
  Example shape (placeholders): `ASNEAREST: "AS" "KST" "2026-11-24 12:00:00Z,MYCALL,JO00AA,DXCALL,JO00BB,1,CALL1,M,120,85,7"` followed by the checksum byte.
- Track messages, sent once per second while tracking, always `Src = Server_Name`, `Dst = ""`, `Data = "AUTO"`:
  - `Track_UDP_WinTest` (`:7864-7875`): `SETAZIMUTH`, `Extra = " 00 " + az.ToString("000")`. Result on the wire: `SETAZIMUTH: "AS" "" "AUTO" 00 123?<CS>`.
  - `Track_UDP_AirScout` (`:7877-7895`): three datagrams per tick: `SETAZIMUTH` (as above), `SETELEVATION` with `" 00 " + el.ToString("000")`, and `SETAZEL` with `" 00 " + az.ToString("F8", Invariant) + " " + el.ToString("F8", Invariant)` (new high-precision form).
  - `"000"` is formatted with the current culture and rounds: azimuth 359.6 becomes `360`; a negative elevation becomes `-05`-style text (bug L9).

### 2.6 Settings involved
| Name | Type | Default | Edited in |
|---|---|---|---|
| `Server_Activate` | bool | false | Options, Network tab (`OptionsDlg.Designer.cs:7528-7538`) |
| `Server_Port` | int | 9872 | Network tab (UDP listen and ASNEAREST reply port) |
| `Server_Name` | string | `AS` | Network tab |
| `Webserver_Port` | int | 9880 | Network tab (see section 3) |
| `Watchlist_SyncWithKST` | bool | false | Watchlist tab (`OptionsDlg.Designer.cs:7755-7756`), enabled only when `Server_Activate` |
| `Track_UDP_WinTest` / `_AirScout` / `_None` | bool x3 | false / false / true | Track tab radio group (`OptionsDlg.cs:2718-2723`) |
| `Track_UDP_WinTest_Port` | int | 9871 | Track tab |
| `Track_UDP_AirScout_Port` | int | 9872 | Track tab |
| `WinTest_Port` | int | (declared at `Properties/Settings.Designer.cs:476`) | not used by any code |

Validation in Options: Server port must differ from Webserver port (`OptionsDlg.cs:2593-2598`); Win-Test and AirScout Track ports must differ (`:2663-2669`). The default Server_Port (9872) equals the default Track_UDP_AirScout_Port (9872); the UI does not compare these two.

### 2.7 Notes
- `AirScout/WinTest.cs` (136 lines) is a stale copy: enum values `SETAZIMUTH`=250, `SETELEVATION`=251, no `SETAZEL`, no watch messages, no `ASNEAREST` special case. It is not listed in `AirScout/AirScout.csproj` (only the `WinTest` project reference at `:2081`), so it is not compiled. Port from `WinTest/WinTest.cs`.
- Rust (suggestion): `UdpSocket` with `socket2` for `SO_REUSEADDR`/`SO_REUSEPORT`, `set_broadcast(true)`, a pure `encode`/`decode` pair with golden byte vectors, and a receive task that polls with a short timeout or uses `tokio::select!` with a cancel token.

---

## 3. Web server (`as-web`)

### 3.1 Public surface and lifecycle
- `bw_Webserver_DoWork` (`AirScout/Webserver.cs:74-193`) is a partial of `MapDlg`. Start args: `TmpDirectory`, `WebserverDirectory`, `PlaneFeedPlugins` (`WebserverStartArgs.cs:9-14`, filled at `MapDlg.cs:2407-2413`).
- Loop: build prefix `http://+:<Webserver_Port>/` (`Webserver.cs:110`), `HttpListener.Start()`, then `GetContext()` in a blocking loop (`:141`). For each request a new `WebserverDeliver : BackgroundWorker` is started (`:151-157`) with a snapshot `AllPlanes = Planes.GetAll(UtcNow, Planes_Position_TTL)` taken before the handler runs (`:142`).
- Any exception: sleep 10 s and re-create the listener (`:174-180`); the old listener is not stopped first.
- Web root: `WebserverDirectory` property (`MapDlg.cs:325-340`): in a debugger the source tree, otherwise `<startup path>/wwwroot`.
- Cancellation is only checked between requests, so the worker cannot stop while idle (bug L7).

### 3.2 Bind address and security (read this first)
- Bind: `http://+:<port>/` is the HttpListener wildcard, meaning **all interfaces**, plain HTTP, no TLS (`Webserver.cs:110`). Default port 9880.
- Authentication: none. No CORS headers, no method check (every HTTP method is treated as GET). Response header is only `Cache-Control: no-cache` (`:1232`).
- On Windows, if the bind fails with access denied (`HttpListenerException.ErrorCode == 5`), AirScout runs `netsh http add urlacl url=http://+:<port>/ user=<current user>` with `Verb = "runas"` (UAC prompt) (`:157-172`). This is the only command execution in the stage 4 code. The command text is fixed except for the port number and the Windows user name.
- **Arbitrary file read.** The static file branch (`:1119-1152`) takes `request.RawUrl`, URL-decodes it with `Uri.UnescapeDataString`, drops the leading `/`, and does `Path.Combine(WebserverDirectory, filename)`. It then serves the file if it exists. Percent-encoded `..` segments are not rejected, and a request such as `//<absolute path>` yields an absolute second argument, which `Path.Combine` returns unchanged. On Linux/macOS (the Mono branch, `:1130-1132`) this reads any file the process can read. HttpListener on Windows normalises some forms, but the code does not rely on it explicitly.
- `GET /settings.json` calls `Properties.Settings.Default.Save()` on every request (`:466`) and returns the whole settings object (own call sign, own coordinates, DX call, paths). Anyone who can reach the port reads them.
- The shipped web UI hard-codes a private LAN IPv4 address and port 9880 as root URL (`AirScout/wwwroot/resources/js/airscout.core.js:563`). The same address appears in the default of `KST_Websocket` (`Settings.settings:773`). The UI only works if served from that exact address.

### 3.3 Request handling order (`WebserverDeliver.OnDoWork`, `Webserver.cs:1106-1252`)
1. `rawurl = UnescapeDataString(RawUrl)`. If it has no `/` or is exactly `/`, it becomes `/index.html` (`:1117-1122`).
2. Strip the leading `/`, convert `/` to `\` unless Mono (`:1124-1133`), cut at `?` (`:1136-1137`), `Path.Combine(WebserverDirectory, filename)`. If the file exists: serve its bytes, content type from the `MimeTypes` map by extension (`:1146-1151`).
3. Otherwise match the API routes below, on the **raw, lower-cased** URL with `StartsWith`, except `/AircraftList.json` (exact, case-sensitive) and `/planes.json` (exact after lower-casing).
4. No match: HTTP 404, body `Not found!`, content type `text/html`.
All handlers return a string that is written as UTF-8. Content type for every API route is `text/json` (not `application/json`). Errors from handlers (bad parameter, not found) come back as **HTTP 200** with a plain-text body starting with `Error` and still labelled `text/json` (for example `:531`, `:573`, `:586`).

Query parsing (all handlers): the part after the first `?` is upper-cased, parsed with `HttpUtility.ParseQueryString`, and keys are looked up in upper case (`CALL`, `LOC`, `BESTCASEELEVATION`, `BAND`, `MYCALL`, `MYLAT`, `MYLON`, `MYLOC`, `DXCALL`, `DXLAT`, `DXLON`, `DXLOC`). So key and value case do not matter. Numbers are parsed with `NumberStyles.Float` and the invariant culture.

### 3.4 Routes
JSON conventions: Newtonsoft.Json with `DateTimeZoneHandling.Utc`, `FloatFormatHandling.String`, `Formatting.Indented` unless stated; enums are written as **integers**; `double` values that are whole numbers are written with `.0`; `NaN`/`Infinity` appear as strings; `DateTime` is ISO 8601 (`2026-11-24T12:00:00Z`, with fractional seconds when non-zero); all public properties and public fields are written in declaration order (base class `LastUpdated` first).

| Route | Query | Handler | Returns |
|---|---|---|---|
| `GET /` or `/index.html` | none | static | wwwroot `index.html`, a meta-refresh redirect to `./dist/index.html` |
| `/version.json` | none | `:451-462` | one JSON string (`Application.ProductVersion`, for example `"1.2.3.4"`), indented |
| `/settings.json` | none | `:464-475` | whole `Properties.Settings.Default` serialised (see 3.6) |
| `/bands.json` | none | `:477-483` | compact array of band names: `["50M","70M","144M","432M","1.2G","2.3G","3.4G","5.7G","10G","24G","47G","76G"]` |
| `/bandvalues.json` | none | `:484-490` | compact `[{"Name":"B50M","Value":50},...]`, `Value` = MHz-style enum value (50, 70, 144, 432, 1296, 2320, 3400, 5760, 10368, 24048, 47088, 76032) |
| `/aircraftcategories.json` | none | `:492-498` | compact `[{"Name":"None","Value":0},{"Name":"Light","Value":1},{"Name":"Medium","Value":2},{"Name":"Heavy","Value":3},{"Name":"Superheavy","Value":4}]` (`AirScout.Core/PlaneCategory.cs:133-146`) |
| `/location.json` | `call` (required), `loc` (optional or `ALL`), `bestcaseelevation` (`true`/`false`, optional) | `:500-561` | one `LocationDesignator`, or an array when `loc=ALL` |
| `/qrv.json` | `call`, `loc` (both required), `band` (optional, name like `144M`, `ALL`) | `:563-619` | one `QRVDesignator`, or an array for `band=ALL` |
| `/elevationpath.json` | `mycall`, `dxcall` (required); `mylat`,`mylon`,`mylocator` / `dxlat`,`dxlon`,`dxloc` (optional, coordinates win over locator) | `:621-763` | one `ElevationPathDesignator` |
| `/propagationpath.json` | same as elevationpath; `band` is sent by the UI but ignored (bug L12) | `:765-921` | one `PropagationPathDesignator` |
| `/nearestplanes.json` | `mycall`,`dxcall` (required), `myloc`,`dxloc`,`band` (optional) | `:923-1048` | array of `PlaneInfo` |
| `/airports.json` | none | `:1050-1056` | compact array of `AirportDesignator` |
| `/planefeedsettings.json` | none | `:1058-1104` | compact array of `{"Name":<feed name>,"Value":<plugin settings object>}` for the up to three configured feeds (`"Key"` is textually replaced by `"Name"`, `:1095`) |
| `/planes.json` | none | `:440-449` | contents of `<Tmp>/planes.json` as written by the JSON writer (3.5) |
| `/AircraftList.json` | none | `:429-438` | contents of `<Tmp>/vrsplanes.json`, written by the VRSWebServer feed plugin (`AirScout.PlaneFeeds.Plugin.VRSWebServer/VRSWebServer.cs:698`) |

Object shapes (names and types as serialised; C# `double`, `int`, `bool`, `string` map to JSON number, number, bool, string):
- `LocationDesignator` (`ScoutBase/ScoutBase.Stations/Location.cs:42-71`): `LastUpdated` (date, from `SQLiteEntry` `SQLiteDatabase/SQLiteDatabase/SQLiteEntry.cs:64`), `Call`, `Loc`, `Lat`, `Lon`, `Source` (int, `GEOSOURCE`), `Hits` (int), `Elevation` (int, metres), `BestCaseElevation` (bool). `GeoLocation` is `[JsonIgnore]`.
- `QRVDesignator` (`ScoutBase/ScoutBase.Stations/QRV.cs:45-50`): `LastUpdated`, `Call`, `Loc`, `Band` (int `BAND`), `AntennaHeight`, `AntennaGain`, `Power`.
- `ElevationPathDesignator` (`ScoutBase/ScoutBase.Elevation/ElevationPath.cs:34-100`): `LastUpdated`, `Bearing12`, `Bearing21`, `Distance` (computed read-only properties are serialised), `Lat1`, `Lon1`, `Lat2`, `Lon2`, `StepWidth`, `Count` (int), `Path` (array of int16), `Valid`, `Selected`, `Location1`, `Location2` (`LocationDesignator`, `null` if coordinates were given), `QRV1`, `QRV2`.
- `PropagationPathDesignator` (`ScoutBase/ScoutBase.Propagation/PropagationPath.cs:28-110`): `LastUpdated`, `Bearing12`, `Bearing21`, `Distance`, `Lat1`, `Lon1`, `h1`, `Lat2`, `Lon2`, `h2`, `QRG`, `Radius`, `F1_Clearance`, `StepWidth`, `Eps1_Min`, `Eps2_Min`, `Valid`, `Selected`, `LocalObstructed`, `LocalObstruction`, `Location1`, `Location2`, `QRV1`, `QRV2`.
- `PlaneInfo` (`AirScout.Core/PlaneInfo.cs:20-72`): `Time`, `Call`, `Reg`, `Hex`, `Lat`, `Lon`, `Alt` (feet), `Alt_m`, `Track`, `Speed` (knots), `Speed_kmh`, `Type`, `Manufacturer`, `Model`, `Category` (int), `From`, `To`, `VSpeed` (int), `IntPoint` (object with `Lat`/`Lon` or `null`), `IntQRB`, `AltDiff`, `Potential` (int), `Eps1`, `Eps2`, `Theta1`, `Theta2`, `Angle`, `Squint`, `SignalStrength`, `Ambiguous`, `Comment`, `OldTime`, `OldLat`, `OldLon`, `OldAlt`, `OldTrack`, `OldSpeed`. `IntQRB` defaults to `double.MaxValue`, `SignalStrength` to `double.MinValue` (serialised as long numbers in exponent form; exact text matters if the UI compares them, for example `Number.MAX_VALUE` in `airscout.wtkst.js:297`).
- `AirportDesignator` (`AirScout.Aircrafts/Airport.cs:34-40`): `LastUpdated`, `ICAO`, `IATA`, `Lat`, `Lon`, `Alt`, `Airport`, `Country`.
- The exact property order for each class should be taken from a recorded response when the Rust side is built (see section 9, proposed golden test: dump each route from a running upstream build).

Route behaviour worth keeping in mind:
- `/location.json` with empty `loc`: returns the latest known location for the call; if there is none, the handler dereferences null (`:541`) and the request never completes (bug L8). With `loc` given, a location not in the database is synthesised from the locator centre (`LocationFind`, `:228-289`). With `bestcaseelevation=true` (or the `Path_BestCaseElevation` setting when the parameter is absent) and a 4-character-precision location, lat/lon are replaced by the highest point in the grid square and `Source` becomes `FROMBEST` (`:268-285`).
- `/elevationpath.json`, `/propagationpath.json`, `/nearestplanes.json` have side effects: `QRVFindOrCreateDefault`, `LocationFindOrCreate`, and storing computed paths in the database (they write to the DB on a GET).
- Elevation model for web lookups: SRTM1, then SRTM3, then GLOBE, only if enabled in settings (`:209-226`). ASTER models are not consulted.
- `/propagationpath.json` and `/nearestplanes.json` compute with the **local** `Path_Band_Settings[band]` (K factor, F1 clearance, max distance).

### 3.5 Files written for the server
- JSON writer (`MapDlg.cs:8463-8545`): every 60 s, if `Planes.GetAll(...)` is non-empty, writes `<Tmp>/planes.json` in a FlightRadar-style layout built by string concatenation (no escaping): `{"full_count":N,"version":<assembly major>` then per plane a line `,"<8 hex digit index>":[hex,lat(F4),lon(F4),track(F0),alt(F0),speed(F0),"","",type,reg,epoch seconds,"","","",0,0,call,0]`, then `}`. Numbers use the invariant culture. When the list is empty the old file stays in place (stale), and if it never existed `/planes.json` fails (bug L8).
- `<Tmp>` is `TmpDirectory` (`MapDlg.cs:143-166`): setting `Tmp_Directory` (default `\Tmp`), relative to `<LocalAppData>/<Company>/<Product>`.

### 3.6 `/settings.json` contract
The handler serialises the whole `AirScout.Properties.Settings` object. The shipped UI reads only these members (grep of `AirScout.Settings.*` in `AirScout/wwwroot/resources/js/`): `Band`, `DXLat`, `DXLon`, `InfoWin_Alt`, `InfoWin_Angle`, `InfoWin_Dist`, `InfoWin_Epsilon`, `InfoWin_Metric`, `InfoWin_Position`, `InfoWin_Speed`, `InfoWin_Squint`, `InfoWin_Time`, `InfoWin_Track`, `InfoWin_Type`, `KST_WebSocket_Timeout`, `KST_Websocket`, `Map_AutoCenter`, `Map_Provider`, `Map_Providers` (list of `{Name,...}`; used for the Leaflet tile URLs, `airscout.ui.js:96-112`), `Map_SmallMarkers`, `MaxLat`, `MaxLon`, `MinLat`, `MinLon`, `MyLat`, `MyLon`, `Path_Band_Settings`, `Path_BestCaseElevation`, `Planes_Filter_Min_Category`, `Planes_MaxAlt`, `Planes_MinAlt`, `Planes_Position_TTL`, `Server_Name`, `Watchlist` (array of `{Call,Loc,Checked,Selected,OutOfRange,Remove}`), `Watchlist_SyncWithKST`.
- `Path_Band_Settings` is a `DataTable` (`AirScout.Core/BandSettings.cs:34-51`). Newtonsoft writes it as an array of row objects with the column names `BAND` (string like `144M`), `K-FACTOR`, `F1-CLEARANCE`, `GROUNDCLEARANCE`, `MAXDISTANCE`, `MAXSQUINT`, `MAXELEVATION` (note the hyphens). The UI looks rows up with `b.BAND == bandstr` and reads `.MAXDISTANCE` (`airscout.core.js:674-678`).
- The rest of the dump is whatever the .NET settings class exposes (possibly including `ApplicationSettingsBase` members such as `Context` and `Properties`). The exact extra members were not verified and no UI code needs them. Decision D4 covers this.

### 3.7 Static files and what JS calls which route
Layout under `AirScout/wwwroot/`:
- `index.html` (508 bytes): meta-refresh and `window.location` redirect to `./dist/index.html`.
- `dist/index.html` (935 lines, title "Welcome to AirScout"): the real UI page. The other `dist/*.html` (about 30 pages), `dist/forms/`, `dist/base/`, `dist/buttons/`, `dist/icons/`, `dist/notifications/` are untouched CoreUI template demo pages. `dist/js/*` (14 files) and `dist/css/*` (8) are CoreUI. `dist/vendors/` (19 MB): `@coreui`, `chart.js`, `jquery`, `leaflet` (plus `leaflet.rotatedMarker.js`), `reconnecting-websocket`, `simplebar`, `tablesorter`. `dist/assets/` (5.7 MB): favicon, 524 icon files (429 brand icons), avatars.
- AirScout code in `resources/`: `css/airscout.css`; `js/`: `scoutbase.core.js` (1172 lines), `scoutbase.stations.js`, `scoutbase.elevation.js`, `scoutbase.propagation.js`, `airscout.aircrafts.js` (6, a stub), `airscout.core.js` (814), `airscout.feeds.js` (324), `airscout.ui.js` (952), `airscout.wtkst.js` (345), `airscout.js` (14, broken stub). Icons: `icons/aircrafts/` (4), `icons/markers/` (44).
- `dist/index.html:266-275` loads the `resources/js` files with `/../../resources/js/...` paths; browsers normalise these to `/resources/js/...`.
- The page loads the Leaflet map and tile images straight from the map provider URLs in `Map_Providers`. The server does not proxy tiles.

Who calls which route (all `$.ajax` with `async: false` at script load, `AirScout.core.js`):
- `settings.json` `:574`, `aircraftcategories.json` `:596`, `planefeedsettings.json` `:627`, `bandvalues.json` `:650`, `airports.json` `:688`.
- `location.json?call=&loc=&bestcaseelevation=` `:717-720`; `location.json?call=&loc=ALL` `:744`.
- `elevationpath.json?mycall&mylat&mylon&dxcall&dxlat&dxlon` `:776`; `propagationpath.json?...&band=` `:797`.
- `bands.json`, `version.json`, `qrv.json`, `nearestplanes.json`, `planes.json`, `AircraftList.json` are not called by the shipped JS (the first three, plus `nearestplanes`, are probably for external clients such as wtKST; unverified).
- Plane data: the browser fetches aircraft itself from the feed URL(s) in `planefeedsettings.json` using JSONP (`airscout.feeds.js:101-120`), so the server never serves live plane data to this UI except through the JSON files above.
- The page also opens a **WebSocket client** to `KST_Websocket` (default `ws://<private LAN address>:9881/AirScout`, `Settings.settings:773`) and reacts to tasks `test`, `set_watchlist`, `get_nearestplanes`, `set_path` (`airscout.wtkst.js:139-160`, `:164-330`). AirScout the application has no WebSocket server; the other end is the wtKST program. Replies carry `{task, from, to, data}`; `get_nearestplanes` answers per call with `{call, loc, data:[{call,cat,int_qrb,int_potential,int_min}]}`. The client-side logic computes nearest planes itself, using `getPropagationPath` over HTTP.

### 3.8 Rust notes (suggestions only)
- axum with `tower-http` `ServeDir`-style serving of embedded assets, or `rust-embed`. The upstream file lookup order (static file first, then API) must not allow traversal; use a fixed asset table.
- Keep route paths, query key names (case-insensitive), the `text/json` content type, HTTP 200 for logical errors and the plain-text `Error: ...` bodies if clients depend on them. Whether to fix these is for stage 6.
- The upstream JSON is produced by reflection. In Rust use explicit structs with `serde` and `#[serde(rename)]` to the exact names above, plus a number formatter that writes whole doubles with `.0`.

---

## 4. CAT (`as-cat`)

### 4.1 Public surface the GUI calls
- `CATWorker : BackgroundWorker` (`AirScout.CAT/CATWorker.cs:30-324`), started with `CATWorkerStartOptions` (`:13-27`: `Name`, `RigType`, `PortName`, `Baudrate` 9600, `DataBits` 8, `Parity`, `StopBits`, `RTS`, `DTR`, `Poll` 100, `Timeout` 500 as class defaults; the real values come from settings).
- Public fields written by the UI thread: `DopplerStrategy`, `RxFrequency`, `TxFrequency` (`:33-36`).
- `CATWorker.SupportedRigs()` (`:49-85`): list of `SupportedRig {Type, Model, CATEngine}` shown in the Options CAT tab (`OptionsDlg.cs:2754-2763`).
- Progress codes: <=0 text, 1 `RIGSTATUS`, 2 `IRig` (rig info) (`CATWorker.cs:99-310`). Consumer: `bw_CAT_ProgressChanged` (`MapDlg.cs:9252-9331`) sets the CAT status light (green ONLINE, orange OFFLINE, red for NOCAT/NOPORT/NORIG/NOTSUITABLE/ERROR), the status text and the tooltip, and, when `TrackStatus != TRACKING`, stores the rig's current RX frequency, mode, split and RIT into `Doppler_DialFreq/DialMode/DialSplit/DialRit` (`:9293-9301`).
- `bw_CATUpdater` (`ScoutBase/ScoutBase.CAT/CATUpdater.cs`, consumer `MapDlg.cs:9203-9246`): downloads the rig definition archive, section 4.5.
- Interface `IRig` (`AirScout.CAT/IRig.cs:30-70`): `Settings`, `CatEngine`, `CatVersion`, `GetStatus`, `GetMode/Split/Rit/Vfo`, `GetRxFrequency/GetTxFrequency`, the matching setters, `SetSimplexMode`, `SetSplitMode(rx, tx, mode)`, `SetDopplerStrategy`, `EnterDoppler`, `LeaveDoppler`.
- Enums (`AirScout.CAT/Enums.cs`): `CATENGINE` {NONE, OMNIRIGX, SCOUTBASE, HAMLIB}, `RIGSTATUS` {NONE 0, NOCAT 1, NOPORT 2, NORIG 4, NOTSUITABLE 8, OFFLINE 16, ONLINE 64, ERROR 128}, `RIGMODE` {NONE, CW, CW_R, LSB, USB, DIG, DIG_R, AM, FM, OTHER}, `RIGSPLIT`, `RIGRIT`, `RIGVFO`, `DOPPLERSTRATEGY` {NONE 0, A 1, B 2, C 4, D 8}, plus serial enums `PARITY`, `STOPBITS`, `FLOWCONTROL`.

### 4.2 Worker loop (`CATWorker.OnDoWork`, `:87-316`)
1. If no rig object: collect supported rigs, find the one whose `Type` equals `StartOptions.RigType`, instantiate `OmniRigX` or `ScoutBaseRig` (`:149-153`; `HAMLIB` is not handled anywhere), copy port settings into `RigSettings` (`:155-170`), report `0 "Opening CAT: Success!"` (`:171`) and `2 Rig` even if no rig matched (bug L14).
2. No rig: report NORIG and sleep 10 s (`:196`).
3. `GetStatus()`; `NONE` means "rig lost" and throws. Report changes of status. When ONLINE, read mode, split, RIT, RX frequency, TX frequency and report rig info (code 2) when any value changed (`:207-282`).
4. Doppler: when `DopplerStrategy` changed, call `SetDopplerStrategy` then `EnterDoppler` (A..D) or `LeaveDoppler` (otherwise) (`:277-293`). While A..D is active, call `Rig.SetSplitMode(RxFrequency, TxFrequency)` on every poll tick (`:296-300`).
5. Exception: report NORIG, drop the rig object (so step 1 runs again), sleep `CAT_Update` (default 500 ms) at the end of each pass (`:308`).
6. On cancel: `LeaveDoppler()` (`:314`) then report `RIGSTATUS.NONE`.

### 4.3 What the Track tab pushes to CAT (Doppler)
In `bw_Track_DoWork` (`MapDlg.cs:8250-8283`), after the rotator outputs, with `Doppler_DialFreq` as `f` and the per-station shifts `MyDoppler` and `DXDoppler` in Hz (computed from successive slant ranges at roughly 1 s spacing, `:8114-8133`):
- Strategy A: RX = f + my + dx, TX = f.
- Strategy B: RX = f, TX = f - my - dx.
- Strategy C: RX = f + my + dx, TX = f - my - dx.
- Strategy D: RX = f + my, TX = f - my.
- All casts `(long)` truncate toward zero. `TrackValues.RXFrequency/TXFrequency` default to 0 (`TrackValues.cs:23-24`).
The UI thread (`MapDlg.cs:8419-8449`) copies strategy, RX and TX into `bw_CAT` only when both frequencies are non-zero. When tracking stops, it sets `bw_CAT.DopplerStrategy = DOPPLER_NONE` (`:8398-8401`), which makes `CATWorker` call `LeaveDoppler` and restore the rig (frequency, mode, split, RIT saved by `EnterDoppler`).
Status of A: the rig must be able to read and write split on/off and write VFO-equal, else `NOTSUITABLE` (`ScoutBaseRig.cs:84-97`, `OmniRigX.cs:253-265`).

### 4.4 Rig engines
- **OmniRig (COM, Windows only)** `AirScout.CAT/OmniRigX.cs`: `Type.GetTypeFromProgID("OmniRig.OmniRigX")`, fallback CLSID (a fixed GUID, `:108-115`), dynamic `Activator.CreateInstance`. Calls used: `InterfaceVersion`, `Rig1..Rig4.RigType`, `.Status`, `.Mode`, `.Freq`, `.FreqA`, `.FreqB`, `.Vfo`, `.Split`, `.Rit`, `.IsParamReadable(p)`, `.IsParamWriteable(p)`, with `OMNIRIGPARAM` and `OMNIRIGSTATUS` enums defined at the top of the file (`:14-60`). Rig type names are `[OmniRig] Rig 1..4` (`:68-71`). Frequencies are limited to 32 bit (`:1228-1230`). `ExtOmniRig.cs` is dead code (nothing references it).
- **ScoutBase.CAT (own serial engine, an OmniRig clone)** `AirScout.CAT/ScoutBaseRig.cs` wraps `ScoutBase.CAT.Rig` (`ScoutBase/ScoutBase.CAT/Rig.cs` 674, `CustomRig.cs` 836, `CommPort.cs` 880, `CommWorker.cs` 148, `RigCommands.cs` 879, `RigCommand.cs`, `RigParam.cs`, `RigSettings.cs`, `OmniRig.cs` 174). Rig definitions are `.ini` files, one per rig, in the rig directory (`RigData.Database.DefaultDatabaseDirectory()`, `ScoutBase.CAT/RigDatabase.cs`), parsed with `IniParser` (`RigCommands.cs:104-139`). `OmniRig.SupportedRigs()` lists the `*.ini` file names (`OmniRig.cs:140-165`). Rig type shown in the UI is `[Serial] <name>` (`ScoutBaseRig.cs:690-700`). The serial I/O is `System.IO.Ports.SerialPort` on a highest-priority thread (`CommPort.cs:83`, `:507-510`, `CommWorker.cs:32`), so the C# code is already cross-platform; it is not Win32-specific.
- **HAMLIB**: enum value only. `CATWorker` neither lists nor creates it; no project references `HamLibSharp` (4.6).
- Frequency writes in `ScoutBaseRig` use `(int)rx` (`ScoutBaseRig.cs:374-389`), so frequencies above 2.147 GHz overflow (not an issue for IF frequencies).
- `SetSplitMode` tries up to 8 different command patterns depending on which parameters the rig definition allows writing (TS-570 style `VfoAB`, IC-746 style `VfoEqual`, FT-100D A/B, FT-817 swap, FT-1000/IC-7610 A+B, marine NMEA) (`ScoutBaseRig.cs:335-444`, `OmniRigX.cs:1113-1221`). `SetSimplexMode` likewise (`ScoutBaseRig.cs:245-333`).

### 4.5 Rig definition download (`bw_CATUpdater`)
- Setting `Rigs_UpdateURL` (ScoutBase.CAT settings), default a **plain `http://`** URL of the upstream project site, with `rigs.zip` appended (`ScoutBase/ScoutBase.CAT/CATUpdater.cs:118-127`). The archive is downloaded with `AutoDecompressionWebClient.DownloadFileIfNewer(..., autounzip: true)` (`ScoutBase/ScoutBase.Core/SupportFunctions.cs:913`) into `<LocalAppData>/<Company>/<Product>/Tmp/RigData` and then every `*.ini` is copied to the rig directory if newer (`CATUpdater.cs:50-95`). Period: `Database_BackgroundUpdate_Period`, default 60 min (`:151`, settings default `60`).
- Security: the INI files define the exact byte strings sent to the radio. They arrive over unauthenticated HTTP. The unzip code was not audited here for path traversal (`SupportFunctions.cs:913-960`).
- Settings: `Database_Directory` default empty (meaning `<LocalAppData>/<Company>/<Product>/RigData`), `Rig_Directory` in the main settings default `\ScoutBase\RigData`, `RigDatabase_Status` (UI light only).

### 4.6 HamLibSharp (`HamLibSharp/`, 10846 lines)
- Third-party .NET binding for the native hamlib C library (README says targets hamlib 1.2, 3.0.1, 3.1; the folder ships 4.1 sources and binaries, LGPL-2.1). P/Invoke into `libhamlib` via `[DllImport(HamLib.dllName)]` (`HamLib.cs:191-231`, `RigNative.cs:72`), with struct mirrors for x86/x64 and several hamlib ABI versions (`x64/RigCapsNative64*.cs`, `x86/*`).
- No `.csproj` or `.cs` outside `HamLibSharp/` references it (checked by search). It is not built into AirScout, so there is no hamlib behaviour to port. Everything about rigctld and hamlib below is new design, not a port.

### 4.7 Settings (CAT)
`AirScout.CAT/Properties/Settings.Designer.cs`, edited in Options, CAT tab (`OptionsDlg.cs:2746-3080`):
| Name | Type | Default |
|---|---|---|
| `CAT_Activate` | bool | false |
| `CAT_Update` | int, ms | 500 (worker poll interval) |
| `CAT_ResponseTimeout` | int | 500 (declared, never read) |
| `CAT_RigType` | string | `[None]` |
| `CAT_PortName` | string | empty |
| `CAT_Baudrate` | int | 9600 (choices 110 to 256000, `OptionsDlg.cs:2793`) |
| `CAT_DataBits` | int | 8 (5 to 8) |
| `CAT_Parity` | `PARITY` | NONE |
| `CAT_StopBits` | `STOPBITS` | ONE |
| `CAT_RTS` / `CAT_DTR` | `FLOWCONTROL` | LOW |
| `CAT_Poll` | int, ms | 100 (rig command poll, passed to the serial engine) |
| `CAT_Timeout` | int, ms | 500 (serial reply timeout) |
Main settings used by the Track/Doppler path: `Doppler_DialFreq` (int64 Hz, 0), `Doppler_DialMode`, `Doppler_DialSplit`, `Doppler_DialRit` (default NONE), `Doppler_Strategy_None/A/B/C/D` (bool; None true), `Doppler_MyDoppler`, `Doppler_DXDoppler`, `Doppler_RXFreq`, `Doppler_TXFreq` (display values).
Port settings are disabled in the UI for OmniRig rigs (`OptionsDlg.cs:2878-2896`). The Doppler group box is enabled only when both `CAT_Activate` and `Track_Activate` are on (`:2622-2631`, `:2896-2899`).

### 4.8 Rust notes (suggestions only)
- Primary path: rigctld over TCP (default `localhost:4532`). The line protocol maps onto `IRig`: `f` read frequency, `F <hz>` set, `m` read mode and passband, `M <mode> <passband>` set, `s` read split, `S <0|1> <txvfo>` set split, `i`/`I` read/set split TX frequency, `j`/`J` RIT, `v`/`V` VFO, `t` PTT. Hamlib mode names (`CW`, `CWR`, `LSB`, `USB`, `PKTUSB`, `AM`, `FM`) map onto `RIGMODE`. Extended response mode (`+f`) and `\dump_state` give capability detection. The `NOTSUITABLE` check for strategy A becomes a split-capability probe.
- Settings that do not apply to rigctld: port name, baud rate, data bits, parity, stop bits, RTS/DTR (rigctld owns the serial port). A rigctld rig type would need new settings (host, port); see decision D5.
- COM OmniRig: `windows` crate `IDispatch` late binding, `cfg(windows)` only (decision D6).
- The ScoutBase serial engine is a large generic interpreter for INI-defined rig dialects. Porting it duplicates what rigctld already does; see decision D7.

---

## 5. Track tab outputs (rotator, files, Doppler)

All of this is `bw_Track_DoWork` (`MapDlg.cs:7992-8328`) plus helpers `:7836-7990` and the UI-side handler `:8352-8450`. The maths (bearing, elevation, slant range, Doppler) is already ported in stage 3c; this section is only about outputs and lifecycle.

### 5.1 Loop and error handling
- Outer `do { ... } while (!Cancel)` loop (`:8011-8330`): create the clients that are enabled (`:8018-8049`), report STOPPED, then the inner loop `while (Track_Activate && !Cancel)` (`:8054`), then sleep `Track_Update` ms (`:8315`).
- Inner loop per second (`Thread.Sleep(1000)` at `:8313`): compute `TrackValues` for `TrackMode == TRACK` (plane `Track_CurrentPlane` looked up at `UtcNow + Track_Offset`) or `SINGLE` (use `Track_SetAz`, `Track_SetEl`) (`:8058-8148`). If azimuth is not NaN and in `[0, 360)` and elevation is not NaN, report TRACKING/SINGLE and call every enabled output in a fixed order: DDE, UDP (WinTest or AirScout), serial AZ, serial AZEL, native file, WSJT file (`:8156-8197`). Then compute Doppler frequencies, console-log, `ReportProgress(3, values)` (`:8250-8287`). SINGLE mode ends with STOPPED (one shot).
- The output try/catch (`:8198-8260`) counts errors only for exceptions whose message starts with `[DDE]:`, `[UDP]:` or `[Serial]:`. After more than 10 such errors it clears the radio-button settings for that output and reports ERROR. The counters are never reset.
- An exception anywhere else in the inner loop (planes lookup, elevation lookup, reporting) breaks out of the inner loop and reports ERROR (`:8305-8309`).
- Cleanup on cancel: disconnect DDE, close UDP, close serial (`:8332-8346`), report NONE.

### 5.2 DDE to Ham Radio Deluxe rotator
`Track_DDE_HRD` (`:7839-7862`), Windows only (NDde, DDEML).
- Client created once per pass: `new DdeClient("HRDRotator", "Position")` (service `HRDRotator`, topic `Position`), `TryConnect()` with the result ignored (`:8020-8022`).
- Per tick: if `0 <= az <= 360`, poke item `PositionData` with the ASCII bytes of `SET-AZ:` + `az.ToString("F1")`; if `0 <= el <= 90`, poke `SET-EL:` + `el.ToString("F1")`. `TryPoke(item, data, format = 1 (CF_TEXT), timeout = 10000 ms)`. No NUL terminator; the `int`/bool result is ignored, so failures are silent (`:7849-7860`). `F1` uses the current culture (decimal comma on German systems; the bug is that HRD may then receive `123,4`).
- Not connected: throws `[DDE]:`-prefixed `InvalidOperationException` (`:7845-7846`). There is no reconnect; if HRD starts after AirScout the client stays disconnected.
- Setting `Track_DDE_HRD` / `Track_DDE_None` (radio group, default None).

### 5.3 GS-232A serial rotator
- Port: `new SerialPort(Track_Serial_Port, Track_Serial_Baudrate, Parity.None, 8, StopBits.One)`, `Handshake.None`, `NewLine = "\r"`, `Encoding.ASCII`, `ReadTimeout = WriteTimeout = 1000` ms, opened once per pass (`:8036-8047`). Defaults: `COM13`, 4800 baud. Options accepts only baud rates 50, 100, 110, 150, 300, 600, 1200, 2400, 4800, 9600, 19200, 38400, 57600, 115200 (`OptionsDlg.cs:2670-2680`).
- `Serial_SendCommand` (`:7897-7913`): `WriteLine(command)` (command plus `\r`), optional `ReadLine()` (up to `\r`), `\n` removed.
- Azimuth only (`Track_SER__GS_232A_AZ`, `:7915-7938`): send `C`, expect a reply starting with `+0` and a parsable number in characters 2..4 (for example `+0123`); wrong format throws `[Serial]:`-prefixed `FormatException`. Then send `M<az:000>` (for example `M123`), no reply read.
- Azimuth and elevation (`Track_SER__GS_232A_AZEL`, `:7940-7962`): send `C2`, expect a reply starting with `+0` (for example `+0123+0045`; only the first number is checked), then `W<az:000> <el:000>` (for example `W123 045`), no reply read.
- `"000"` rounds and is culture-dependent; `359.6` produces `360`, negative elevation produces a leading minus (bug L9). The loop only rejects azimuth outside `[0,360)` before rounding.
- Settings: `Track_Serial_Port` (string, `COM13`), `Track_Serial_Baudrate` (int, 4800), `Track_Serial_GS232_AZ`, `Track_Serial_GS232_AZEL`, `Track_Serial_None` (bool radio group, default None). Edited in the Track tab (`OptionsDlg.cs:2705-2710`).
- Platform: `System.IO.Ports` (already cross-platform in .NET). Rust equivalent: the `serialport` crate (suggestion). On Linux/macOS the port name is a device path, so `Track_Serial_Port` should be a free-text field (it already is a text box).

### 5.4 azel.dat files
Both write `<TmpDirectory>/azel.dat`, overwritten every tick, `StreamWriter` default (UTF-8, no BOM, `Environment.NewLine`), numbers invariant (`:7964-7990`).
- Native (`Track_File_Native`): one line `az,el` with `F1`, for example `123.4,5.6`.
- WSJT (`Track_File_WSJT`): four lines with `utc = DateTime.UtcNow.ToString("HH:mm:ss")`: `<utc>,0,0,Moon`, `<utc>,0,0,Sun`, `<utc>,<az F1>,<el F1>,Source`, `0,  0, 0,  0, 0,Doppler, R`. The az/el are in the "Source" line (comment `:7976`).
- Path: `TmpDirectory` = setting `Tmp_Directory` (default `\Tmp`) under `<LocalAppData>/<Company>/<Product>` (`MapDlg.cs:143-166`). The file is rewritten in place, not atomically; a reader can see an empty or half-written file.
- Settings: `Track_File_Native`, `Track_File_WSJT`, `Track_File_None` (default None).

### 5.5 Other Track settings
| Name | Type | Default | Notes |
|---|---|---|---|
| `Track_Activate` | bool | false | Track tab checkbox; enables the worker (`OptionsDlg.cs:2682-2703`) |
| `Track_Offset` | int, s | 0 | time offset added to UtcNow for the plane lookup (`MapDlg.cs:8059`) |
| `Track_Update` | int, ms | 1000 | only the sleep between outer passes (see bug L11) |
| `Track_SetAz`, `Track_SetEl` | double | 0, 0 | set by right-click on the DX call needle (`MapDlg.cs:5528-5545`) and by `ASSHOWPATH` |
| `Track_CurrentPlane` | string | empty | hex of the plane clicked while playing (`MapDlg.cs:5029`, `:5509`) |
| `Doppler_*` | see 4.7 | | |
Status lights: `TRK` (TRACKSTATUS) and `ROT` (ROTSTATUS) with colours NONE grey, STOPPED dark grey, SINGLE/TRACKING green, ERROR red/yellow (`MapDlg.cs:8362-8412`).

### 5.6 Rust notes (suggestions only)
- One `track` task with an `mpsc` input of `TrackSample` and a list of `Box<dyn TrackOutput>` (`UdpWinTest`, `UdpAirScout`, `Gs232Az`, `Gs232AzEl`, `FileNative`, `FileWsjt`, `DdeHrd`), each with `connect/send/close` so errors are counted per output and reconnect is possible.
- Honour the Windows-only `as-dde` with `#[cfg(windows)]`; hide the DDE option elsewhere.

---

## 6. DDE (`as-dde`) in one place
- Only consumer is `Track_DDE_HRD` (5.2). NDDE (`NDDE/NDde/Source/NDde`) is a managed DDEML wrapper (`DdeClient.cs` 1890 lines, `DdeServer.cs` 1177). `using NDde.Server` in `MapDlg.cs:49-51` is unused.
- DDEML calls needed for a Rust client: `DdeInitialize`, `DdeCreateStringHandle` (service, topic, item), `DdeConnect`, `DdeClientTransaction(XTYP_POKE, CF_TEXT, 10 s)`, `DdeDisconnect`, `DdeUninitialize`. Suggestion: call them directly through the `windows` crate (user32). `cfg(windows)` only.

---

## 7. SpecLab spectrum input (adjacent, not CAT or Win-Test)
The Spectrum tab's signal levels do not come from CAT or Win-Test. `bw_SpecLab_Receive` (`MapDlg.cs:7625-7790`) polls an HTTP URL once per `SpecLab_Update` s (minimum 1 s) while in LIFE + FORWARD mode: `GET <SpecLab_URL>/<SpecLab_FileName>?f1=<f1>?f2=<f2>` (second separator is `?`, bug L13), expects a JSON array text, parses header items by index (`h[1]` epoch seconds, `h[13]` bin count) and a list of values after replacing `,` with `;` and `.` with `,` and calling `Double.Parse` with the current culture (works only in a decimal-comma culture, bug L13). The maximum is reported to the UI and, when a nearest plane exists, stored as a `SignalLevelDesignator` and bulk-inserted every 60 entries (`SignalData.Database.SignalLevelBulkInsertOrUpdateIfNewer`).
Settings (SpecLab tab, `OptionsDlg.cs:2602-2617`): `SpecLab_Enabled` false, `SpecLab_URL` `http://localhost:10080/`, `SpecLab_FileName` `_fft_expt.json`, `SpecLab_Update` 1 (s, double), `SpecLab_F1` 400, `SpecLab_F2` 1600 (Hz, 0..3000, F1 < F2).
Recommendation: not part of the stage 4 integrations; treat as a small add-on for 4a or 4b if the owner wants it (decision D11). It reuses `as-signals`.

---

## 8. Listening sockets, secrets and platform parts

### 8.1 Every socket and process
| What | Direction | Address, port | Default state | Auth |
|---|---|---|---|---|
| Win-Test UDP receiver | listen | UDP `0.0.0.0:Server_Port` (9872), `SO_REUSEADDR` | off (`Server_Activate`) | none; checksum not enforced |
| Web server | listen | TCP `http://+:Webserver_Port/` (9880), all interfaces | off (`Server_Activate`) | none, no TLS |
| ASNEAREST reply | send | UDP broadcast `255.255.255.255:Server_Port` | with receiver | none |
| Track UDP | send | UDP broadcast `255.255.255.255:9871` (WinTest) or `:9872` (AirScout) | off | none |
| GS-232A | open | serial port | off | n/a |
| HRD DDE | connect | local DDE service `HRDRotator`/`Position` | off | Windows DDE |
| SpecLab | connect | outbound HTTP to `SpecLab_URL` (localhost 10080) | off | none |
| Rig definitions | connect | outbound plain HTTP to the upstream site | on when CAT is active | none |
| CAT serial / OmniRig | open | serial port / COM OmniRig | off | n/a |
| `netsh` elevation | exec | Windows only, on HTTP bind failure | n/a | UAC |
| Browser UI | outbound from browser | `KST_Websocket` (default private LAN address, port 9881), map tile hosts, plane feed URLs | n/a | none |

### 8.2 Credentials and keys in code
No credentials, tokens or API keys were found in the stage 4 code. Checked: `AirScout/Webserver.cs`, `WinTest/*.cs`, `AirScout.CAT/*.cs`, `ScoutBase/ScoutBase.CAT/*.cs`, the Track/WinTest regions of `MapDlg.cs`, and the settings and plane-feed plugin settings (by name). One false positive: `AirScout/MapDlg.cs:2981` reads the assembly public key token (identity check). The `AirScout_GetKey_URL` setting (AirScout `Settings.settings`) belongs to the map/feeds code and is outside this survey.
Private LAN address hard-coded (value deliberately not copied here): `AirScout/wwwroot/resources/js/airscout.core.js:563` and `AirScout/Properties/Settings.settings:773` (`KST_Websocket` default).

### 8.3 Platform-specific parts and cross-platform equivalents (suggestions)
- Win32/COM: OmniRig ActiveX (`AirScout.CAT/OmniRigX.cs`), DDEML (`NDDE`), `netsh http add urlacl`, `WindowsIdentity.GetCurrent()` (`Webserver.cs:111`), user32 `SetForegroundWindow`/`ShowWindow` in `ASShowPath` (`MapDlg.cs:7160-7172`).
- Portable already: `System.IO.Ports.SerialPort` (GS-232A, ScoutBase.CAT), `UdpClient`, `HttpListener` (Mono has one).
- Rust: `serialport` crate for serial, `std::net::UdpSocket` plus `socket2`, `axum` (embedded server), rigctld over `tokio::net::TcpStream`, `windows` crate for COM/DDE under `cfg(windows)`.
- The unreferenced HamLibSharp suggests dropping native hamlib; rigctld reaches the same hamlib backends.

---

## 9. Suspected bugs (to be logged as logic findings, not fixed)
L1. `MapDlg.cs:7036-7041`: if the UDP bind fails the error is only logged and the receive loop continues. `u.Receive` on an unbound socket then throws `InvalidOperationException` (not `SocketException`), which is caught in the outer handler (`:7061-7064`) without any delay: a busy loop that floods the log.
L2. `WinTest/WinTest.cs:69-103` and `MapDlg.cs:7054-7056`: no bounds checks while parsing (one log line per stray datagram) and the checksum result `HasChecksum` is never evaluated, so the "checksum" gives no protection. `Src` is not validated.
L3. `MapDlg.cs:7077-7092`: `ASSHOWPATH` pauses, switches the tab and sets `PathMode = SINGLE` before it validates the data; a 4-character locator throws in `Substring(0, 6)` and leaves the GUI in that state. Callsign and locator validation happen after the first state changes.
L4. `MapDlg.cs:7258-7260`: for `ASSETPATH` the propagation path is calculated with `Properties.Settings.Default.Band` (frequency, K factor, F1 clearance) and the QRV defaults at `:7223-7236` use the same current band, while the plane search at `:7265` uses `Path_Band_Settings[band]` of the requested band. A request for a band other than the GUI band mixes two bands. `(BAND)qrg` at `:7199-7203` never throws (casts any integer), so an unknown frequency becomes an undefined band and `Path_Band_Settings[band]` fails.
L5. `MapDlg.cs:7295-7301`: broadcast send without `EnableBroadcast` (see 2.2). The same code is used in `Track_UDP_*` (`:8024-8033`).
L6. `MapDlg.cs:7864-7868`: garbled null check in `Track_UDP_WinTest` (`if ((client == null) || (ip == null)) if (client == null) throw ...`); `ip == null` is not detected. Harmless in practice.
L7. `Webserver.cs:140-141` and `MapDlg.cs:2484-2501`: the web server worker blocks in `GetContext`, so cancellation is only noticed after the next request; and `bw_Webserver` is missing from the stop list, so port or activation changes in Options do not take effect without a restart (see 1). `Webserver.cs:171-185`: an exception creates a new listener without stopping the old one.
L8. `Webserver.cs:541`, `:559` (null `ld` dereference when a call has no stored location and `loc` is empty), `:429-447` (missing `vrsplanes.json` or `planes.json`; the JSON writer only writes when planes exist, `MapDlg.cs:8473-8478`): exceptions in `OnDoWork` end the worker without closing the response, so the client waits until its own timeout.
L9. `MapDlg.cs:7870`, `:7886`, `:7890`, `:7934`, `:7958`: `ToString("000")` is culture-dependent and rounds (359.6 gives `360`, which a GS-232 rotator rejects), and a negative elevation produces `-05`-style text. The only check before is `Azimuth < 360` on the unrounded value (`:8142-8146`). `Track_DDE_HRD` `F1` is culture-dependent (`:7852`, `:7858`).
L10. `MapDlg.cs:8011-8049` and `:8305-8319`: the outer loop re-creates the DDE client, UDP client and serial port on every pass without closing the previous ones. Any exception that breaks the inner loop (`:8305-8309`) leads to a second `SerialPort.Open()` on a still-open port, which fails every pass (permanent ERROR until restart) and leaks UDP sockets.
L11. `MapDlg.cs:8313-8315`: `Track_Update` is not the update interval; the inner loop always sleeps 1000 ms and `Track_Update` is only the pause between outer passes.
L12. `Webserver.cs:765-921`: `/propagationpath.json` ignores the `band` query parameter that `airscout.core.js:797` sends; it always uses `Properties.Settings.Default.Band`. Every other route that has a `BAND` parameter reads it.
L13. `MapDlg.cs:7667` (`?f1=..?f2=..` instead of `&`) and `:7692-7695` (decimal separator replaced and parsed with current culture). Already in the upstream-bugs list of the GUI survey for `?f2`.
L14. `AirScout.CAT/CATWorker.cs:168-173`: "Opening CAT: Success!" and `ReportProgress(2, Rig)` are reported even when no supported rig matched (`Rig` stays null). `:277-293`: `EnterDoppler()` returns false when the rig is not ONLINE, but `olddoppler` is already updated, so it is never retried and the rig state to restore stays zero.
L15. `AirScout.CAT/CATWorker.cs:296-300`: while a Doppler strategy is active, `SetSplitMode` runs on every poll tick even when the frequencies did not change (chatty on the CAT line). `RxFrequency`/`TxFrequency` are `long` fields written from another thread without a lock.
L16. `AirScout.CAT/OmniRigX.cs:130-141`, `:229-237` and `:1312-1380`: the version tests are impossible conditions (`< 0x210 && > 0x299`), so `Is2x()` is always false (rigs 3 and 4 unreachable) and `SupportedRigs()` lists rigs only for interface version `0x101`; an OmniRig V2 yields an empty list. A real interface version `0x110` throws "unsupported" (`:141`).
L17. `AirScout.CAT/OmniRigX.cs:1032` and `:1123`: `IntSetSimplexMode`/`IntSetSplitMode` assign the C# `RIGMODE` enum to `rig.Mode` instead of the `OMNIRIGPARAM` constant (the converting `SetMode` is at `:640-647`), so `EnterDoppler` (which passes the saved mode) writes a wrong mode value on OmniRig rigs.
L18. `MapDlg.cs:8236-8247`: the serial error branch increments `serialerr` twice per error (`:8234` and `:8246`), so the output is switched off after 6 errors, not 11. DDE and UDP errors are only counted when thrown with the prefix; real socket failures and `TryPoke` failures are never counted (`:7849-7860`, `:8021`). `ddeerr`, `udperr`, `serialerr` are never reset, so isolated errors add up over a session.
L19. `MapDlg.cs:8470-8517` (JSON writer): strings are concatenated into JSON without escaping and the file is rewritten in place every 60 s, not atomically.
L20. `AirScout/Webserver.cs:1119-1152`: path traversal and absolute path read (see 3.2). `:466` `Settings.Save()` on a GET.
L21. `MapDlg.cs:8438-8447`: the four Doppler booleans are checked with independent `if`s (last one wins), while `:8252-8277` uses `else if` (first wins). With more than one flag set the two places disagree. The Rust `DopplerStrategy` enum avoids this.
L22. `ScoutBaseRig.cs:374-389`, `:402-418`: frequencies cast to `int` (overflow above 2.147 GHz).

---

## 10. Proposed split of stage 4

All sub-stages end with green CI on three OS; items marked [win] are `cfg(windows)` and tested only on the Windows runner.

**4a: Win-Test UDP, Track core, UDP and file outputs.**
- `as-wintest`: `WTMESSAGES`, `wtMessage` encode/decode, golden byte vectors (hand-built from 2.3, plus a small C# dump if a harness is available).
- Receiver worker with `Server_Activate/Port/Name`, the four dispatch handlers (`ASSHOWPATH`, `ASSETPATH`, `ASWATCHLIST`, `ASADDWATCH`, `ASREMOVEWATCH`), `ASNEAREST` reply.
- Options Network tab (UDP part), Watchlist-sync checkbox hook.
- Track worker core with the `TrackOutput` trait, UDP WinTest and AirScout outputs, azel.dat native and WSJT, ROT/TRK status lights, Options Track tab (everything except serial and DDE).
- Rationale: no platform dependencies and no hardware; fully testable on loopback; sets the worker and output patterns for the rest; closes most of the 3c deferred list.

**4b: CAT with rigctld and the Doppler push.**
- `as-cat`: `IRig`, `RigStatus` etc., rigctld TCP client, `CatWorker` (poll, reconnect, Doppler enter/leave), settings for CAT (host/port plus the existing CAT fields kept for compatibility), the status light, Options CAT tab, Doppler hand-off from the Track worker.
- Test against a fake rigctld (TCP stub), no hardware.
- Rationale: independent of 4a except the Doppler hand-off; the biggest user-visible value after Win-Test.

**4c: GS-232A serial rotator and HRD DDE.**
- `serialport`-based `Gs232Az`/`Gs232AzEl` outputs with the exact command strings of 5.3, tested with a pty pair or a mock trait.
- `as-dde` [win]: DDEML client, `Track_DDE_HRD` output.
- Options Track tab serial and DDE groups.
- Rationale: hardware-dependent and OS-dependent items grouped so 4a/4b stay portable; can be reordered freely.

**4d: Web server.**
- `as-web` with the embedded asset subset, all routes of 3.4 with golden JSON from recordings of upstream, the JSON writer files `planes.json` (and reading `vrsplanes.json`), Options Network tab (HTTP part), start/stop in the Options cycle.
- Rationale: largest and the one with security impact; needs `Settings` JSON shape and the stage 3 databases/propagation, so it is best done after 4a to reuse the lifecycle code. Needs decisions D1 to D4 first.

**4e: Optional platform extras (only if the owner wants them).**
- [win] OmniRig COM engine (`as-cat` feature `omnirig`).
- ScoutBase.CAT INI rig engine and rig-definition download (decision D7).
- Native hamlib FFI (decision D8).
- SpecLab input if not done in 4a/4b (decision D11).
- Rationale: each is large, has low reach, or duplicates 4b.

### Open decisions (owner)
D1. Web server bind address default.
- Options: (a) `0.0.0.0` (upstream, LAN visible); (b) `127.0.0.1` default with a "allow LAN" setting; (c) `127.0.0.1` only.
- Recommendation: (b). The data returned includes own call and position, and upstream has an arbitrary-file-read bug. Add an optional shared token for LAN mode. The UDP receiver stays on `0.0.0.0` because Win-Test broadcasts need it, with the checksum enforced as a setting (D9).

D2. Which web assets to ship.
- Options: (a) the full 29 MB `wwwroot`; (b) only `index.html`, `dist/index.html` and the vendors and CSS/JS it references plus `resources/`; (c) rewrite the UI.
- Recommendation: (b) embedded in the binary. Also change the two hard-coded private addresses (`airscout.core.js:563`, `Settings.settings:773`) to relative or configurable values; that is a small UI change, not an API change, and the UI is otherwise unusable. Needs a yes from the owner because the spec says "unchanged".

D3. Web server lifecycle.
- Options: restart with the other workers on Options OK (differs from upstream, which needs an app restart), or keep upstream behaviour.
- Recommendation: restart with the workers; it is the obvious intent of `Server_Activate`/`Webserver_Port`.

D4. `/settings.json` content.
- Options: (a) full settings dump (upstream); (b) a curated object with exactly the ~35 fields in 3.6 plus `Path_Band_Settings` as the row array.
- Recommendation: (b), same names. It avoids leaking unrelated settings and does not need the `.NET` base class members. No `Settings.Save()` side effect.

D5. rigctld settings model.
- Options: add `CAT_Engine` (rigctld/OmniRig/…), `CAT_Host`, `CAT_Port`; keep serial fields only for native engines.
- Recommendation: add them; hide the serial fields unless a native engine is built.

D6. OmniRig (Windows).
- Options: include in 4e; skip.
- Recommendation: include in 4e later, behind `cfg(windows)`. Upstream only works with OmniRig V1.x (L16); decide whether the Rust version should support V2 properly (recommended) or mirror V1.19.

D7. ScoutBase.CAT native INI engine and rig-definition download.
- Options: port (about 4.5k lines of C# plus an INI parser plus the HTTP download); drop; replace by rigctld only.
- Recommendation: drop for now. rigctld covers the radios and the download is unauthenticated HTTP. Keep the Options rig list free of `[Serial]` entries.

D8. Native hamlib FFI.
- Options: include (needs libhamlib at build or run time on every OS); skip.
- Recommendation: skip; upstream does not use it (4.6) and rigctld is the stated primary path.

D9. Win-Test UDP security.
- Options: accept everything (upstream); require a valid checksum; also restrict to a source subnet.
- Recommendation: validate the checksum by default with an "accept unchecked" escape hatch, and log the source address. Needs a real Win-Test/wtKST capture to confirm every sender sets a valid checksum.

D10. Port defaults.
- `Server_Port` and `Track_UDP_AirScout_Port` both default to 9872 upstream.
- Options: keep 1:1; or change one default.
- Recommendation: keep values 1:1 for settings compatibility, add a validation warning in Options for the collision.

D11. SpecLab input.
- Options: include in 4a/4b; defer; drop.
- Recommendation: defer to 4e. It is neither CAT nor Win-Test, needs the `?f2` and culture fixes in a deliberate way, and is HTTP polling only.

D12. Track behaviours to keep 1:1 or fix now.
- Items: `Track_Update` semantics (L11), `000` rounding and negative elevation (L9), error counters (L18), reconnect (L10).
- Recommendation: port with a correct `connect/reconnect` structure in 4a (it is architecture, not maths), but keep rounding and clamping exactly as upstream and log them as stage 6 findings.

## Decisions (project owner, 2026-11-24)

- D1 web bind address: as upstream, all interfaces (`0.0.0.0`), no authentication, no TLS.
  The server stays off by default (`Server_Activate`, as upstream). The static-file path
  traversal (absolute paths, `..`) is closed regardless; it is not prediction behaviour.
- D2 web assets: embed the subset `index.html` actually uses (its JS/CSS and vendor libraries),
  not the CoreUI demo pages or unused icons. The hard-coded private LAN IP becomes the page's
  own origin. Routes and JSON shapes stay unchanged.
- D3 web restart: restart with the other workers on Options OK (controller call).
- D4 `/settings.json`: a curated object with the fields the web UI reads, same names and
  formats, no `Save()` side effect (controller call).
- D5 rigctld: add engine, host and port settings (controller call).
- D6 OmniRig: included, `cfg(windows)` only, with working V2 support.
- D7 ScoutBase.CAT INI engine and its plain-HTTP rig download: dropped (controller call).
- D8 native hamlib FFI: included, optional (cargo feature, off by default; rigctld TCP stays
  the primary path).
- D9 Win-Test UDP checksum: ignored, as upstream (1:1 port); logged as a logic finding for stage 6.
- D10 port defaults: kept 1:1; Options warns when two enabled sockets share a port (controller call).
- D11 SpecLab input: deferred (not in stage 4).
- D12 Track behaviour: correct connect/reconnect structure in 4a; rounding, clamping and error
  counters ported 1:1 and logged for stage 6 (controller call).

Sub-stages: 4a Win-Test UDP + Track worker core (UDP and azel.dat outputs, Network and Track
tabs); 4b `as-cat` over rigctld + Doppler push + CAT tab + optional native hamlib; 4c GS-232A
serial rotator, `as-dde` HRD and OmniRig (Windows); 4d `as-web`.
