# Plane feeds

| Feed | Default URL or host | Enabled by default | Desktop disclaimer |
|------|---------------------|--------------------|--------------------|
| OpenSky | `https://opensky-network.org/api/states/all` | Yes | No |
| ADSBOne | `https://api.adsb.one/v2/point/%MYLAT%/%MYLON%/250` | Yes | No |
| Readsb | `http://localhost/tar1090/data/aircraft.json` | Yes | No |
| Dump1090 | `http://localhost:8080/data/aircraft.json` | Yes | No |
| AirScoutServer | `http://localhost:9880/planes.json` | Yes | No |
| VRSWebServer | `http://localhost:7780/VirtualRadar/AircraftList.json?fSBnd=%MINLAT%&fNBnd=%MAXLAT%&fWBnd=%MINLON%&fEBnd=%MAXLON%` | Yes | No |
| BinCraft | `http://44.149.131.185/tar1090/data/aircraft.binCraft.zst` (AMPRNet) | No | No |
| PublicBinCraft | `http://localhost` (self-hosted readsb/tar1090) | No | Yes |
| ADSBSupport | `https://airscout.hamradio4.me/planes?fSBnd=%MINLAT%&fNBnd=%MAXLAT%&fWBnd=%MINLON%&fEBnd=%MAXLON%&age=300` | No | Yes |
| RTL1090 | `localhost:31001` (Beast or AVR over TCP) | Yes | No |
| FlexJSON | None (configure a JSON URL) | Yes | Yes |

Restricted feeds are off by default and show a disclaimer in the plane server.
The desktop applies the disclaimers listed in the table. OpenSky supports
anonymous access or your OAuth client. ADSBSupport requires credentials.
Plane feed requests use the configured coverage area.

## Multiple feeds

The desktop runs up to three feeds at once (the C# `Planes_PlaneFeed1/2/3`
slots). Options → Planes shows three selectors "Feed 1/2/3"; slots 2 and 3
offer a `[none]` entry and default to it. One worker runs per distinct
non-empty slot, so a slot that repeats an earlier feed adds nothing, and every
feed writes into the same aircraft cache. The status line reports each active
feed's state (for example `OpenSky: 123 planes | VRS: error`) without
credentials. A feed with an unaccepted disclaimer keeps its slot empty until
you accept it.
