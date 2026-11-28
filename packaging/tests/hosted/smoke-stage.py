#!/usr/bin/env python3
"""Exercise the six packaged commands in a temporary profile without a display."""
import argparse
import http.client
import json
import os
from pathlib import Path
import re
import signal
import shutil
import subprocess
import sys
import tempfile
import time

PROGRAMS = ("airscout", "airscout-planeserver", "airscout-cli", "catcheck",
            "elevation-tile-gen", "maptile-gen")
SERVING = re.compile(r"serving /planes\.json on http://127\.0\.0\.1:(\d+)/")


def clean_environment(root):
    # Windows needs its system directory. Never inherit proxy, display, loader
    # overrides, user profile paths, CI secrets, or the runner's PATH.
    env = {"PATH": os.defpath, "LC_ALL": "C"}
    if os.name == "nt":
        for name in ("SystemRoot", "WINDIR"):
            if name in os.environ:
                env[name] = os.environ[name]
    for keys, directory in (
        (("HOME", "USERPROFILE"), "home"),
        (("XDG_CONFIG_HOME", "APPDATA"), "config"),
        (("XDG_DATA_HOME", "LOCALAPPDATA"), "data"),
        (("XDG_CACHE_HOME",), "cache"),
        (("XDG_RUNTIME_DIR",), "runtime"),
        (("TMPDIR", "TMP", "TEMP"), "tmp"),
    ):
        path = root / directory
        path.mkdir(mode=0o700)
        env.update(dict.fromkeys(keys, str(path)))
    return env


def planes_response(port):
    client = http.client.HTTPConnection("127.0.0.1", port, timeout=1)
    try:
        client.request("GET", "/planes.json")
        response = client.getresponse()
        if response.status != 200:
            raise ValueError(f"/planes.json returned HTTP {response.status}")
        body = json.loads(response.read(65537))
    finally:
        client.close()
    if (not isinstance(body, dict) or type(body.get("version")) is not int
            or type(body.get("full_count")) is not int
            or body["version"] != 1 or body["full_count"] != 0):
        raise ValueError(f"Unexpected /planes.json shape: {body!r}")


def smoke_daemon(command, root, env, flatpak=False):
    config = root / "config" / "planeserver.toml"
    settings = 'server_active = true\nfeeds = ["[none]", "[none]", "[none]"]\n'
    config.write_text(settings, encoding="utf-8")
    log = root / "daemon.log"
    # AppRun may regenerate GTK caches before starting PlaneServer under QEMU.
    readiness_timeout = 18
    run_for = 15
    args = ["--headless", "--config", str(config), "--bind", "127.0.0.1",
            "--port", "0", "--data", env["XDG_DATA_HOME"], "--run-for", str(run_for)]
    if flatpak:
        # The sandbox has a private /tmp. Create its config there and keep
        # its data isolated from the Flatpak application profile.
        args = ["-eu", "-c", 'mkdir /tmp/airscout-smoke; '
                'printf "%s" "$1" > /tmp/airscout-smoke/planeserver.toml; shift; '
                'exec /app/bin/airscout-planeserver "$@"',
                "smoke", settings, "--headless", "--bind", "127.0.0.1",
                "--port", "0", "--run-for", str(run_for), "--config",
                "/tmp/airscout-smoke/planeserver.toml", "--data", "/tmp/airscout-smoke"]
    with log.open("wb") as output:
        process = subprocess.Popen(command + args, env=env, cwd=root,
                                   stdin=subprocess.DEVNULL, stdout=output,
                                   stderr=subprocess.STDOUT, start_new_session=os.name != "nt")
        try:
            deadline = time.monotonic() + readiness_timeout
            while time.monotonic() < deadline:
                match = SERVING.search(log.read_text(encoding="utf-8", errors="replace"))
                if match:
                    try:
                        planes_response(int(match[1]))
                    except (OSError, http.client.HTTPException):
                        pass
                    else:
                        break
                if process.poll() is not None:
                    raise ValueError(f"PlaneServer exited before replying ({process.returncode})")
                time.sleep(0.1)
            else:
                raise ValueError("PlaneServer HTTP request timed out")
            if process.wait(timeout=run_for + 3) != 0:
                raise ValueError(f"PlaneServer exited with status {process.returncode}")
        except (ValueError, subprocess.TimeoutExpired):
            print(log.read_text(encoding="utf-8", errors="replace"), file=sys.stderr)
            raise
        finally:
            if process.poll() is None:
                if os.name == "nt":
                    process.kill()
                else:
                    try:
                        os.killpg(process.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                process.wait(timeout=5)


def check_command(command, flag, expected, root, env):
    result = subprocess.run(command + [flag], env=env, cwd=root, stdin=subprocess.DEVNULL,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=20,
                            encoding="utf-8", errors="replace")
    if result.returncode or not re.search(expected, result.stdout):
        raise ValueError(f"{command[0]} {flag} exited {result.returncode}: {result.stdout}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("bin_dir", type=Path)
    parser.add_argument("version")
    launcher_group = parser.add_mutually_exclusive_group()
    launcher_group.add_argument("--apprun", type=Path, help="invoke commands through AppRun --tool")
    launcher_group.add_argument("--flatpak-ref", help="invoke commands from an installed Flatpak ref")
    args = parser.parse_args()
    root_dir = args.bin_dir.resolve()
    suffix = ".exe" if os.name == "nt" else ""
    commands = {name: [str(root_dir / (name + suffix))] for name in PROGRAMS}
    if args.apprun:
        launcher = str(args.apprun.resolve())
        commands = {name: [launcher, "--tool", name] for name in PROGRAMS}

    daemon = commands["airscout-planeserver"]
    if args.flatpak_ref:
        flatpak_user_dir = os.environ.get("FLATPAK_USER_DIR")
        if not flatpak_user_dir:
            raise ValueError("Set FLATPAK_USER_DIR for --flatpak-ref")
        flatpak = shutil.which("flatpak")
        if not flatpak:
            raise ValueError("Flatpak is not installed")
        prefix = [flatpak, "run", "--user", "--nosocket=wayland", "--nosocket=x11",
                  "--nosocket=fallback-x11"]
        commands = {name: prefix + [f"--command={name}", args.flatpak_ref] for name in PROGRAMS}
        daemon = prefix + ["--command=/bin/sh", args.flatpak_ref]

    with tempfile.TemporaryDirectory(prefix="airscout-smoke-") as scratch:
        root = Path(scratch)
        env = clean_environment(root)
        if args.flatpak_ref:
            env["FLATPAK_USER_DIR"] = flatpak_user_dir
        version = rf"(?<![\w.]){re.escape(args.version)}(?![\w.])"
        if args.apprun:
            check_command([launcher], "--version", version, root, env)
        for command in commands.values():
            check_command(command, "--version", version, root, env)
            check_command(command, "--help", r"(?i)usage:", root, env)
        smoke_daemon(daemon, root, env, flatpak=bool(args.flatpak_ref))
    print("Verified six command-line tools and the headless PlaneServer")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, subprocess.TimeoutExpired) as error:
        sys.exit(f"smoke-stage: {error}")
