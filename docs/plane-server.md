# Plane server

The plane server combines up to three feeds and serves `/planes.json`.
Run it with a window and tray icon or as a headless daemon.

```sh
airscout-planeserver
airscout-planeserver --headless
```

Set `feeds` in `planeserver.toml`, next to `settings.toml`. The default
list is empty. The window saves changes when you click Apply. Set per-feed
options in `[feed_settings.<FeedName>]`. The plane server requires acceptance
for any feed that is off by default or has a disclaimer. In headless mode,
read the disclaimer in the window, then add the feed to
`feeds_disclaimer_accepted` in the config file.

The HTTP listener is on by default, with no authentication. Anyone who can
reach it can read the collected aircraft positions. Responses omit credentials
and configuration. Turn off `server_active` to disable the listener. For
local-only access, set `bind = "127.0.0.1"` or pass `--bind 127.0.0.1`.
To use the server as an AirScout feed, select AirScoutServer and set
`http://<host>:9872/planes.json`.

On Linux the tray icon needs `libayatana-appindicator3` or `libappindicator3`
at runtime. On GNOME it needs the "AppIndicator and KStatusNotifierItem Support"
extension. On Wayland, closing the window quits the server. Use `--headless`
to run it without a desktop. See `airscout-planeserver --help` for options.

See [plane feeds](plane-feeds.md) for defaults and disclaimers.
