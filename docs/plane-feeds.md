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
