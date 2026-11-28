# Network sockets

The desktop's UDP receiver and HTTP server are off by default. In Options →
Network, "Activate Network Server" enables both.

| Connection | Default address and port | When active |
|------------|--------------------------|-------------|
| Win-Test receiver | `0.0.0.0:9872`, inbound UDP | Network server enabled |
| ASNEAREST reply | `255.255.255.255:9872`, UDP broadcast | In response to `ASSETPATH` |
| Win-Test tracking | `255.255.255.255:9871`, UDP broadcast | Win-Test UDP tracking selected and active |
| AirScout tracking | `255.255.255.255:9872`, UDP broadcast | AirScout UDP tracking selected and active |
| Desktop web server | `0.0.0.0:9880`, inbound TCP (HTTP) | Network server enabled |
| CAT rigctld | `localhost:4532`, outbound TCP | CAT enabled with rigctld |
| Spectrum Lab | `http://localhost:10080/`, outbound HTTP | Spectrum Lab enabled |
| Map preloader | `www.airscout.eu:80`, outbound HTTP | Map preloading enabled with OpenStreetMap and GLOBE, SRTM3 or SRTM1 |
| Plane server | `0.0.0.0:9872`, inbound TCP (HTTP) | `server_active` is on (default) |

The desktop web UI opens at `http://localhost:9880/`. The browser connects
to wtKST at `ws://localhost:9881/AirScout` by default. AirScout sends outbound
requests for configured feeds, data updates and map tiles. The plane server
connects only to configured feeds apart from its HTTP listener.

For the developer tools, `wintest-sim` sends to `255.255.255.255:9872` and,
with `--listen`, listens on `0.0.0.0:9872` over UDP. `catcheck` uses
`localhost:4532` over TCP with rigctld. The standalone elevation coverage
download uses `http://www.airscout.eu/downloads/ScoutBase/1/ElevationData`
as its base URL. The tile generators and `horizon-gen` work locally.

The desktop network server accepts unauthenticated UDP commands. Its plain
HTTP interface exposes your call, position, DX station, watchlist and plane
feed URL. The served plane feed URL and KST websocket URL have their
username, password and query string removed; a credential placed in the URL
path is served as configured and still reaches the network. Requests can
compute and store paths. ASNEAREST replies broadcast calls and locators.
Keep the network server off or filter its ports on an untrusted network.

CAT commands go to `rigctld` in plain text.
