import base64
import json
import os
import re
import subprocess
import textwrap
import tempfile
import unittest
from pathlib import Path

from test_linux import ROOT, run, workspace_version, yaml


CONTAINER = ROOT / "packaging/container"
WORKFLOW = ROOT / ".github/workflows/release-container.yml"
DOCKERFILE = CONTAINER / "Dockerfile"
REPO = "ghcr.io/hamradioonline/airscout-planeserver"
SOURCE_URL = "https://github.com/HamRadioOnline/AirScout-RS"
PLATFORMS = {"linux/amd64", "linux/arm64", "linux/arm/v7"}
# Slug (stage dir + tools.lock key) -> platform, arch token (tarball + internal
# ref), and the Rust target each stage must declare.
SLUGS = {
    "linux-x86_64": {"platform": "linux/amd64", "arch": "amd64",
                     "target": "x86_64-unknown-linux-gnu"},
    "linux-arm64": {"platform": "linux/arm64", "arch": "arm64",
                    "target": "aarch64-unknown-linux-gnu"},
    "linux-armhf": {"platform": "linux/arm/v7", "arch": "armv7",
                    "target": "armv7-unknown-linux-gnueabihf"},
}


def container_stage_root(root, slugs=("linux-x86_64", "linux-arm64", "linux-armhf")):
    """Write one stage dir per slug (bin/airscout-planeserver + legal files +
    stage.json), mirroring the layout the workflow extracts from stage-* artifacts."""
    stage_root = Path(root) / "stage"
    for slug in slugs:
        slug_dir = stage_root / slug
        (slug_dir / "bin").mkdir(parents=True)
        binary = slug_dir / "bin" / "airscout-planeserver"
        binary.write_text("#!/bin/sh\nexit 0\n")
        binary.chmod(0o755)
        for name in ("README.md", "LICENSE", "NOTICE"):
            (slug_dir / name).write_text(f"{name} fixture\n")
        (slug_dir / "stage.json").write_text(json.dumps({
            "target": SLUGS[slug]["target"], "slug": slug,
            "version": workspace_version(), "glibc_floor": "2.35",
        }))
    return stage_root


def _registry_key(ref):
    return ref.replace("/", "_").replace(":", "_")


def container_toolchain(directory):
    """Put a Docker/Buildx/Skopeo CLI shim on PATH."""
    bin_dir = Path(directory) / "bin"
    bin_dir.mkdir(parents=True)
    fake = bin_dir / "docker"
    fake.write_text(r'''#!/usr/bin/env python3
import argparse
import base64
import hashlib
import io
import json
import os
from pathlib import Path
import re
import sys
import tarfile

STATE = Path(os.environ["FAKE_DOCKER_STATE"])
MANIFEST = "application/vnd.oci.image.manifest.v1+json"
INDEX = "application/vnd.oci.image.index.v1+json"
HOST_USER = f"{os.getuid()}:{os.getgid()}"


def die(message, status=2):
    print(f"docker: {message}", file=sys.stderr)
    sys.exit(status)


def sha(data):
    return "sha256:" + hashlib.sha256(data).hexdigest()


def key(ref):
    return ref.replace("/", "_").replace(":", "_")


def registry(ref, value=None):
    path = STATE / "registry" / (key(ref) + ".json")
    if value is not None:
        path.parent.mkdir(exist_ok=True)
        path.write_text(json.dumps({"raw": value}))
    elif path.exists():
        return json.loads(path.read_text())["raw"]
    else:
        error = os.environ.get("FAKE_REGISTRY_ERROR", "manifest unknown")
        die(f'reading manifest {ref.rsplit(":", 1)[-1]} in {ref}: {error}', 1)


def log(*parts):
    with (STATE / "tags.log").open("a") as file:
        print(*parts, file=file)


def option(args, name):
    if name not in args or args.index(name) + 1 == len(args):
        die(f"missing {name}")
    return args[args.index(name) + 1]


def build(args):
    platform = option(args, "--platform")
    output = option(args, "--output")
    if output.startswith("type=docker,"):
        die("Docker exporter requires a docker-container builder in this fixture")
    if not output.startswith("type=oci,oci-mediatypes=true,dest="):
        die("buildx requires an OCI exporter")
    dockerfile = Path(option(args, "-f"))
    if not dockerfile.is_file() or not os.access(
        Path(args[-1]) / "bin/airscout-planeserver", os.X_OK
    ):
        die("missing Dockerfile or staged executable")
    dest = Path(output.split("dest=", 1)[1])
    with tarfile.open(dest, "w") as archive:
        def add(path, value):
            data = json.dumps(value).encode()
            digest = sha(data)
            path = path or "blobs/" + digest.replace(":", "/")
            member = tarfile.TarInfo(path)
            member.size = len(data)
            archive.addfile(member, io.BytesIO(data))
            return {"digest": digest, "size": member.size}

        os_name, architecture, *variant = platform.split("/")
        image_config = {"os": os_name, "architecture": architecture}
        if variant:
            image_config["variant"] = variant[0]
        config = add(None, image_config)
        config["mediaType"] = "application/vnd.oci.image.config.v1+json"
        image = add(None, {"schemaVersion": 2, "mediaType": MANIFEST,
                           "config": config, "layers": []})
        image["mediaType"] = MANIFEST
        add("index.json", {"schemaVersion": 2, "mediaType": INDEX, "manifests": [image]})
        add("oci-layout", {"imageLayoutVersion": "1.0.0"})
    log("build", platform, output)


def from_oci_archive(path):
    with tarfile.open(path) as archive:
        index = json.load(archive.extractfile("index.json"))
        digest = index["manifests"][0]["digest"]
        data = archive.extractfile("blobs/" + digest.replace(":", "/")).read()
        if sha(data) != digest:
            die("OCI manifest digest mismatch")
        return json.loads(data), digest


def check_registry_auth(path, user):
    if not os.environ.get("FAKE_GHCR_TOKEN"):
        return
    # The shim cannot chown to the container UID on an unprivileged host.
    # Model ownership separately from the real 0600 file it created.
    owner = STATE / "auth-owner"
    if not path.is_file() or not owner.is_file():
        die("GHCR credentials are missing")
    if user != "0:0" and user != owner.read_text():
        die("opening GHCR auth file: permission denied")
    if path.stat().st_mode & 0o777 != 0o600:
        die("expected a private auth file")
    auth = json.loads(path.read_text()).get("auths", {}).get("ghcr.io", {}).get("auth")
    expected = os.environ["FAKE_GHCR_ACTOR"] + ":" + os.environ["FAKE_GHCR_TOKEN"]
    if auth != base64.b64encode(expected.encode()).decode():
        die("invalid GHCR credentials")


def skopeo(args, command):
    if option(args, "--pull") != "never" or "--rm" not in args:
        die("Skopeo must run in a pinned disposable container")
    pin = args[args.index("/usr/bin/skopeo") + 1]
    if not re.fullmatch(r"quay\.io/skopeo/stable:[^@]+@sha256:[0-9a-f]{64}", pin):
        die("Skopeo image must be pinned")
    user = option(args, "--user") if "--user" in args else "0:0"
    with (STATE / "skopeo.log").open("a") as file:
        print(command[0], user, file=file)
    mounts = {}
    for i, arg in enumerate(args):
        if arg == "--volume":
            local, remote, *_ = args[i + 1].split(":")
            mounts[remote] = Path(local)

    def local(path):
        root, tail = path.lstrip("/").split("/", 1)
        return mounts["/" + root] / tail

    def check_skopeo_auth():
        if os.environ.get("FAKE_GHCR_TOKEN"):
            if option(args, "--env") != "REGISTRY_AUTH_FILE" or not os.environ.get(
                "REGISTRY_AUTH_FILE"
            ):
                die("Skopeo is missing REGISTRY_AUTH_FILE")
            check_registry_auth(local(os.environ["REGISTRY_AUTH_FILE"]), user)

    if command[0] == "login":
        parser = argparse.ArgumentParser(allow_abbrev=False)
        parser.add_argument("--compat-auth-file", required=True)
        parser.add_argument("--username", required=True)
        parser.add_argument("--password-stdin", required=True, action="store_true")
        parser.add_argument("registry", choices=["ghcr.io"])
        login = parser.parse_args(command[1:])
        # Skopeo scans stdin line by line and concatenates the lines.
        password = "".join(sys.stdin.read().splitlines()) if "-i" in args else ""
        if (not password or password != os.environ.get("FAKE_GHCR_TOKEN")
                or login.username != os.environ.get("FAKE_GHCR_ACTOR")):
            die("invalid GHCR login")
        path = local(login.compat_auth_file)
        auth = base64.b64encode(f"{login.username}:{password}".encode()).decode()
        with os.fdopen(os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), "w") as file:
            json.dump({"auths": {login.registry: {"auth": auth}}}, file)
        (STATE / "auth-owner").write_text(user)
        print("Login Succeeded!")
    elif command[:2] == ["inspect", "--raw"]:
        check_skopeo_auth()
        print(json.dumps(registry(command[2].removeprefix("docker://"))))
    elif command[0] == "copy":
        source, dest = command[-2:]
        manifest, digest = from_oci_archive(local(source.removeprefix("oci-archive:")))
        if dest.startswith("docker-archive:"):
            if option(args, "--network") != "none":
                die("OCI smoke conversion must be offline")
            archive, ref = dest.removeprefix("docker-archive:").split(":", 1)
            with tarfile.open(local(source.removeprefix("oci-archive:"))) as oci:
                config_digest = manifest["config"]["digest"]
                config = oci.extractfile("blobs/" + config_digest.replace(":", "/")).read()
            saved_manifest = [{"Config": "config.json", "RepoTags": [ref], "Layers": []}]
            with tarfile.open(local(archive), "w") as saved:
                for name, data in (("manifest.json", json.dumps(saved_manifest).encode()),
                                   ("config.json", config)):
                    member = tarfile.TarInfo(name)
                    member.size = len(data)
                    saved.addfile(member, io.BytesIO(data))
        elif dest.startswith("docker://"):
            check_skopeo_auth()
            ref = dest.removeprefix("docker://")
            if "--preserve-digests" not in command or not ref.endswith("@" + digest):
                die("publication must preserve the OCI image manifest digest")
            registry(ref, manifest)
            log("push", ref)
        else:
            die("unsupported Skopeo destination")
    else:
        die("unsupported Skopeo command")


def create_index(args):
    if os.environ.get("FAKE_GHCR_TOKEN"):
        check_registry_auth(Path(os.environ["DOCKER_CONFIG"]) / "config.json", HOST_USER)
    if len(args) % 2:
        die("expected Buildx option/value pairs")
    tags, sources, annotations = [], [], {}
    for flag, value in zip(args[::2], args[1::2]):
        if flag == "--tag":
            tags.append(value)
        elif flag == "--file":
            sources.append(json.loads(Path(value).read_text()))
        elif flag == "--annotation":
            if not value.startswith("index:"):
                die("unsupported annotation type")
            annotation_key, annotation_value = value.removeprefix("index:").split("=", 1)
            annotations[annotation_key] = annotation_value
        else:
            die("unexpected imagetools create argument")
    if not tags or not sources:
        die("imagetools create requires tags and source references")
    repo = tags[0].rsplit(":", 1)[0]
    for source in sources:
        if "mediaType" in source:
            die("a typed --file descriptor cannot infer a source digest reference")
        source["mediaType"] = registry(repo + "@" + source["digest"])["mediaType"]
    index = {"schemaVersion": 2, "mediaType": INDEX, "manifests": sources,
             "annotations": annotations}
    for ref in tags:
        registry(ref, index)
        log("create-tag", ref)


def main(args):
    if args[:2] == ["buildx", "build"]:
        build(args[2:])
    elif args[:3] == ["buildx", "imagetools", "create"]:
        create_index(args[3:])
    elif args[0] == "pull":
        return
    elif args[0] == "load":
        path = Path(option(args, "--input"))
        if not path.is_file():
            die("archive not found")
        with tarfile.open(path) as archive:
            saved, = json.load(archive.extractfile("manifest.json"))
            image, = saved["RepoTags"]
            digest = sha(archive.extractfile(saved["Config"]).read())
        (STATE / key(image)).write_text(digest)
        print("Loaded image:", image.removeprefix("docker.io/library/"))
    elif args[0] == "run" and "/usr/bin/skopeo" in args:
        skopeo(args, args[args.index("/usr/bin/skopeo") + 2:])
    elif args[:2] == ["run", "--detach"]:
        for expected in ("--pull", "never", "--network", "none",
                         "type=volume,target=/config",
                         "type=volume,target=/data", "--run-for"):
            if expected not in args:
                die(f"missing smoke-test argument {expected}")
        if not (STATE / key(args[-3])).is_file():
            die("smoke test must use the loaded image")
        print(option(args, "--platform"))
    elif args[:2] == ["exec", "-i"]:
        platform = args[2]
        probe = sys.stdin.read()
        for expected in ("${DISPLAY+x}", "id -u", "id -g", "ldd ", "not found",
                         "touch /config/", "/data/", "127.0.0.1/9872", "200"):
            if expected not in probe:
                die(f"missing smoke assertion {expected}")
        if os.environ.get("FAKE_SMOKE_FAILURE") == platform:
            die("headless probe failed")
        log("smoke", platform)
    elif args[0] == "wait":
        print(0)
    elif args[0] in ("rm", "logs") or args[:2] == ["image", "rm"]:
        return
    elif args[0] in ("tag", "push") and any("@sha256:" in arg for arg in args[1:]):
        die("tag and push do not accept digest references")
    else:
        die("unexpected Docker arguments: " + " ".join(args))


if __name__ == "__main__":
    main(sys.argv[1:])
''')
    fake.chmod(0o755)
    return dict(os.environ, PATH=f"{bin_dir}:{os.environ['PATH']}")


def toolchain_env(directory):
    """Fake docker env with a fresh, isolated state directory."""
    state = Path(directory) / "state"
    state.mkdir(parents=True, exist_ok=True)
    env = container_toolchain(directory)
    env["FAKE_DOCKER_STATE"] = str(state)
    env.pop("GITHUB_TOKEN", None)
    return env


def create_tags(directory):
    log = Path(directory) / "state" / "tags.log"
    if not log.exists():
        return set()
    return {line.split()[1].rsplit(":", 1)[-1] for line in log.read_text().splitlines()
            if line.startswith("create-tag ")}


def seed_registry(directory, ref, raw):
    """Seed a tag's manifest so publish.sh's immutability/ordering guards see it."""
    state = Path(directory) / "state"
    encoded = _registry_key(ref)
    path = state / "registry" / f"{encoded}.json"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps({"raw": {"schemaVersion": 2, **raw}}))


class ContainerTests(unittest.TestCase):
    def test_platforms_and_tags(self):
        version = workspace_version()
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            output, env = self.build_candidate(root)
            sidecar = json.loads((output / "result.json").read_text())
            self.assertEqual(set(sidecar["platforms"]), PLATFORMS)
            self.assertEqual(sidecar["version"], version)
            self.assertEqual(set(sidecar["images"]),
                             {"linux-amd64", "linux-arm64", "linux-armv7"})
            index = json.loads((output / "index.json").read_text())
            self.assertEqual({"/".join(filter(None, (p["os"], p["architecture"],
                                                     p.get("variant"))))
                              for p in (m["platform"] for m in index["manifests"])}, PLATFORMS)

            for channel, suffix, tags in (("stable", None, {version, "latest"}),
                                          ("beta", "rc.1", {"beta"}),
                                          ("alpha", None, set())):
                with self.subTest(channel=channel):
                    (root / "state/tags.log").unlink(missing_ok=True)
                    result = self.publish(output, env, channel, suffix)
                    self.assertEqual(result.returncode, int(channel == "alpha"), result.stderr)
                    self.assertEqual(create_tags(root), tags)

    def test_headless_entrypoint(self):
        dockerfile = DOCKERFILE.read_text()
        # Non-root user with the fixed UID/GID 9872.
        self.assertRegex(dockerfile, r"(?m)^USER 9872(:9872)?$")
        self.assertIn("--uid 9872", dockerfile)
        self.assertIn("--gid 9872", dockerfile)
        # Two writable mount paths.
        self.assertEqual(dockerfile.count("VOLUME ["), 1)
        self.assertIn('["/config", "/data"]', dockerfile)
        # The headless port.
        self.assertIn("EXPOSE 9872/tcp", dockerfile)
        # Exec-form entrypoint with the headless contract and the two mounts.
        self.assertIn('ENTRYPOINT ["/usr/local/bin/airscout-planeserver", "--headless", '
                      '"--config", "/config/planeserver.toml", "--data", "/data"]',
                      dockerfile)
        # OCI labels carry the organization and the source URL.
        self.assertIn("org.opencontainers.image.source", dockerfile)
        self.assertIn(SOURCE_URL, dockerfile)
        self.assertIn("HamRadioOnline", dockerfile)

    def test_build_rejects_missing_stage(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            stage_root = container_stage_root(root, slugs=("linux-x86_64",))
            env = toolchain_env(root)
            result = run("bash", CONTAINER / "build.sh", stage_root, root / "out",
                         env=env, check=False)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse((root / "out").exists())

    def test_build_rejects_wrong_arch_binary(self):
        # A stage whose stage.json target disagrees with its slug must be refused.
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            stage_root = container_stage_root(root)
            (stage_root / "linux-x86_64" / "stage.json").write_text(json.dumps({
                "target": "aarch64-unknown-linux-gnu", "slug": "linux-x86_64",
                "version": workspace_version(), "glibc_floor": "2.35",
            }))
            env = toolchain_env(root)
            result = run("bash", CONTAINER / "build.sh", stage_root, root / "out",
                         env=env, check=False)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse((root / "out").exists())

    def build_candidate(self, root):
        stage = container_stage_root(root)
        env = toolchain_env(root)
        output = root / "candidate"
        run("bash", CONTAINER / "build.sh", stage, output, env=env)
        return output, env

    def publish(self, output, env, channel="stable", suffix=None):
        args = [output, channel, workspace_version()]
        if suffix:
            args.append(f"beta-v{workspace_version()}-{suffix}")
        return run("bash", CONTAINER / "publish.sh", *args, env=env, check=False)

    def test_publish_immutable_retry_idempotent(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            output, env = self.build_candidate(root)
            first = self.publish(output, env)
            self.assertEqual(first.returncode, 0, first.stderr)
            log = root / "state/tags.log"
            previous = log.read_text()
            second = self.publish(output, env)
            self.assertEqual(second.returncode, 0, second.stderr)
            self.assertEqual(log.read_text(), previous)

    def test_build_requires_compatible_glibc(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            stage = container_stage_root(root)
            metadata = stage / "linux-x86_64/stage.json"
            data = json.loads(metadata.read_text())
            data["glibc_floor"] = "99.0"
            metadata.write_text(json.dumps(data))
            output = root / "out"
            result = run("bash", CONTAINER / "build.sh", stage, output,
                         env=toolchain_env(root), check=False)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("glibc", result.stderr)
            self.assertFalse(output.exists())

    def test_build_smokes_each_oci_archive_offline(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            output, _ = self.build_candidate(root)
            index = json.loads((output / "index.json").read_text())
            self.assertEqual(len(index["manifests"]), 3)
            log = (root / "state/tags.log").read_text().splitlines()
            conversions = (root / "state/skopeo.log").read_text().splitlines()
            self.assertEqual(conversions, [f"copy {os.getuid()}:{os.getgid()}"] * 3)
            for platform in PLATFORMS:
                with self.subTest(platform=platform):
                    self.assertTrue(any(line.startswith(f"build {platform} ")
                                        and "type=oci" in line for line in log))
                    self.assertIn(f"smoke {platform}", log)

    def test_readiness_budget_fits_smoke_run(self):
        script = (CONTAINER / "build.sh").read_text()
        run_window = re.search(r"--run-for (\d+)", script)
        verifier = (CONTAINER.parent / "tests/hosted/verify-container.sh").read_text()
        probe = verifier.split("<<'SMOKE'", 1)[1].split("\nSMOKE", 1)[0]
        budget = re.search(r"while \(\( SECONDS < (\d+) \)\); do", probe)
        self.assertIsNotNone(run_window)
        self.assertIsNotNone(budget, "expected an elapsed-time budget for the readiness loop")
        self.assertGreaterEqual(int(budget[1]), 10)
        attempt = re.search(r"if timeout (\d+) ", probe)
        interval = re.search(r"sleep ([\d.]+)", probe)
        self.assertLess(int(budget[1]) + int(attempt[1]) + float(interval[1]),
                        int(run_window[1]))

    def test_publish_authenticated(self):
        token = "fake-ghcr-token-for-container-tests"
        actor = "fake-release-runner"
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            output, env = self.build_candidate(root)
            skopeo_log = root / "state/skopeo.log"
            skopeo_log.unlink()
            scratch = root / "publish-work"
            scratch.mkdir()
            result = self.publish(output, dict(env, GITHUB_TOKEN=token, GITHUB_ACTOR=actor,
                                               FAKE_GHCR_TOKEN=token, FAKE_GHCR_ACTOR=actor,
                                               TMPDIR=str(scratch)))
            log = (root / "state/tags.log").read_text()
            commands = skopeo_log.read_text()
            encoded = base64.b64encode(f"{actor}:{token}".encode()).decode()
            for artifact in (result.stdout, result.stderr, log, commands,
                             (output / "result.json").read_text()):
                self.assertNotIn(token, artifact)
                self.assertNotIn(encoded, artifact)
            self.assertEqual(list(scratch.iterdir()), [])
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(create_tags(root), {workspace_version(), "latest"})
            user = f"{os.getuid()}:{os.getgid()}"
            self.assertEqual(commands.splitlines(), [f"login {user}"]
                             + [f"inspect {user}"] * 2 + [f"copy {user}"] * 3)
            published = Path(str(output) + ".result.json").read_text()
            self.assertNotIn(token, published)
            self.assertNotIn(encoded, published)
            self.assertIs(json.loads(published)["published"], True)

    def test_pin_source_digests(self):
        def check_pin_sources(entries):
            for name, entry in entries.items():
                if not isinstance(entry, dict):
                    continue
                if "source" in entry:
                    with self.subTest(pin=name):
                        source = entry["source"]
                        self.assertTrue(source)
                        if "sha256:" in source:
                            self.assertEqual(source.split("sha256:", 1)[1],
                                             entry["digest"].removeprefix("sha256:"))
                elif "digest" in entry or "sha256" in entry:
                    self.fail(f"missing source for {name}")
                else:
                    check_pin_sources(entry)

        check_pin_sources(json.loads((ROOT / "packaging/tools.lock.json").read_text()))

    def test_failed_smoke_leaves_no_candidate(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            output = root / "out"
            env = dict(toolchain_env(root), FAKE_SMOKE_FAILURE="linux/arm/v7")
            result = run("bash", CONTAINER / "build.sh", container_stage_root(root),
                         output, env=env, check=False)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse(output.exists())

    def test_registry_lookup_errors_fail_before_push(self):
        for error in ("401 Unauthorized", "429 Too Many Requests",
                      "500 Internal Server Error", "TLS handshake timeout"):
            with self.subTest(error=error), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                output, env = self.build_candidate(root)
                result = self.publish(output, dict(env, FAKE_REGISTRY_ERROR=error))
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(error, result.stderr)
                self.assertNotIn("push ", (root / "state/tags.log").read_text())

    def test_mutable_unknown_version_fails_before_push(self):
        for annotation in (None, "bogus"):
            with self.subTest(annotation=annotation), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                output, env = self.build_candidate(root)
                seed_registry(root, f"{REPO}:latest", {
                    "mediaType": "application/vnd.oci.image.index.v1+json",
                    "manifests": [],
                    "annotations": ({"org.opencontainers.image.version": annotation}
                                    if annotation else {}),
                })
                result = self.publish(output, env)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("version", result.stderr)
                self.assertNotIn("push ", (root / "state/tags.log").read_text())

    def test_beta_orders_prereleases_and_records_full_tag(self):
        for older, newer in (("rc1", "rc2"), ("rc.2", "rc.10"), ("rc", "rc.1"),
                             ("1", "rc")):
            with self.subTest(older=older, newer=newer), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                output, env = self.build_candidate(root)
                result = self.publish(output, env, "beta", newer)
                self.assertEqual(result.returncode, 0, result.stderr)
                ref = root / "state/registry" / f"{_registry_key(REPO + ':beta')}.json"
                self.assertEqual(json.loads(ref.read_text())["raw"]["annotations"]
                                 ["org.opencontainers.image.version"],
                                 f"beta-v{workspace_version()}-{newer}")
                log = root / "state/tags.log"
                previous = log.read_text()
                result = self.publish(output, env, "beta", older)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(log.read_text(), previous)

    def test_publish_immutable_mismatch_fails(self):
        version = workspace_version()
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            stage_root = container_stage_root(root)
            env = toolchain_env(root)
            output = root / "candidate"
            run("bash", CONTAINER / "build.sh", stage_root, output, env=env, check=False)
            seed_registry(root, f"{REPO}:{version}", {
                "mediaType": "application/vnd.oci.image.index.v1+json",
                "manifests": [{"digest": "sha256:" + "a" * 64, "size": 1,
                               "platform": {"os": "linux", "architecture": "amd64"}}],
                "annotations": {"org.opencontainers.image.version": version},
            })
            result = run("bash", CONTAINER / "publish.sh", output, "stable", version,
                         env=env, check=False)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("immutable", result.stderr)
            self.assertNotIn("push ", (root / "state/tags.log").read_text())

    def test_older_stable_still_publishes_immutable_version(self):
        version = workspace_version()
        newer_version = f"{int(version.split('.')[0]) + 1}.0.0"
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            stage_root = container_stage_root(root)
            env = toolchain_env(root)
            output = root / "candidate"
            run("bash", CONTAINER / "build.sh", stage_root, output, env=env, check=False)
            seed_registry(root, f"{REPO}:latest", {
                "mediaType": "application/vnd.oci.image.index.v1+json",
                "manifests": [],
                "annotations": {"org.opencontainers.image.version": newer_version},
            })
            result = run("bash", CONTAINER / "publish.sh", output, "stable", version,
                         env=env, check=False)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(create_tags(root), {version})

    def test_fake_docker_rejects_invalid_forms(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            env = toolchain_env(root)
            docker = root / "bin" / "docker"
            for args in (
                ("buildx", "build"),                      # missing required flags
                ("buildx", "build", "--platform=linux/amd64"),  # no output/metadata
                ("load",),                                # missing --input
                ("tag", "sha256:1234", f"{REPO}@sha256:1234"),
                ("push", f"{REPO}@sha256:1234"),
                ("buildx", "build", "--platform", "linux/amd64",
                 "--output", "type=docker,dest=foo.tar"),
                ("buildx", "imagetools", "create", "--tag", "x", "--annotation",
                 "org.opencontainers.image.version=0", "--file", "unused"),
                ("tag", "one"),                           # missing REPO:TAG
                ("push",),                                # missing REF
                ("buildx", "imagetools", "create", "--tag", "x"),  # no source refs
                ("buildx", "imagetools", "inspect"),      # missing reference
            ):
                with self.subTest(args=args):
                    result = run(docker, *args, env=env, check=False)
                    self.assertEqual(result.returncode, 2, result.stderr)
                    self.assertIn("docker:", result.stderr)

    def test_workflow_structure_stdlib(self):
        workflow = WORKFLOW.read_text()
        self.assertRegex(workflow, r"(?ms)^on:\n  workflow_call:\n    inputs:\n"
                         r"      tag:\n        required: true\n        type: string")
        self.assertNotRegex(workflow, r"(?m)^    secrets:")
        build, publish = workflow.split("\n  publish:\n")
        self.assertNotIn("secrets.", build)
        self.assertNotIn("packages: write", build)
        self.assertRegex(build, r"(?m)docker/setup-qemu-action@[^\n]+\n"
                         r"        with:\n          image: ")
        self.assertIn("docker/setup-buildx-action@", build)
        self.assertIn("driver: docker-container", build)
        self.assertIn("image=", build)
        self.assertIn("environment: release", publish)
        self.assertIn("GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}", publish)
        self.assertIn("packages: write", publish)
        self.assertIn("if: ${{ inputs.publish && !startsWith(inputs.tag, 'alpha-') }}", publish)
        self.assertIn("group: container-publish", publish)

    @unittest.skipUnless(yaml, "PyYAML unavailable")
    def test_workflow_structure_pyyaml(self):
        parsed = yaml.load(WORKFLOW.read_text(), Loader=yaml.BaseLoader)
        workflow_call = parsed["on"]["workflow_call"]
        self.assertEqual(workflow_call["inputs"]["tag"], {"required": "true", "type": "string"})
        self.assertEqual(workflow_call["inputs"]["publish"],
                         {"required": "false", "type": "boolean", "default": "false"})
        self.assertNotIn("secrets", workflow_call)
        jobs = parsed["jobs"]
        self.assertEqual(set(jobs), {"build", "publish", "verify-published"})
        build, publish = jobs["build"], jobs["publish"]
        self.assertNotIn("packages", build.get("permissions", {}))
        self.assertNotIn("secrets.", str(build))
        for action, required in (("setup-qemu-action", {"image"}),
                                 ("setup-buildx-action", {"driver", "driver-opts"})):
            step = next(s for s in build["steps"]
                        if s.get("uses", "").startswith(f"docker/{action}@"))
            self.assertEqual(set(step["with"]), required)
        self.assertEqual(publish["environment"], "release")
        self.assertEqual(publish["permissions"]["packages"], "write")
        self.assertEqual(publish["if"],
                         "${{ inputs.publish && !startsWith(inputs.tag, 'alpha-') }}")
        self.assertEqual(publish["concurrency"]["group"], "container-publish")
        step = next(s for s in publish["steps"] if s.get("id") == "publish")
        self.assertEqual(step["env"]["GITHUB_TOKEN"], "${{ secrets.GITHUB_TOKEN }}")

    def test_lockfile_container_pins(self):
        lock = json.loads((ROOT / "packaging/tools.lock.json").read_text())
        for name in ("binfmt", "buildkit", "skopeo"):
            with self.subTest(tool=name):
                pin = lock["container-tools"][name]
                self.assertRegex(pin["digest"], r"^sha256:[0-9a-f]{64}$")
                self.assertTrue(pin["source"].startswith("https://"))
        bases = lock["container-base"]
        self.assertEqual(set(bases), set(SLUGS))
        for slug, meta in SLUGS.items():
            with self.subTest(slug=slug):
                pin = bases[slug]
                self.assertEqual(pin["platform"], meta["platform"])
                self.assertRegex(pin["digest"], r"^sha256:[0-9a-f]{64}$")
                self.assertNotEqual(pin["digest"], "sha256:" + "0" * 64)
                self.assertRegex(pin["glibc"], r"^\d+\.\d+$")
                self.assertIn("source", pin)
                self.assertTrue(pin["source"].startswith("https://"))
                self.assertTrue(pin["source"].endswith("/" + pin["digest"]))
        # Each platform has its own digest, even where the image tag is shared.
        self.assertIn("ubuntu:22.04", bases["linux-x86_64"]["url"])
        self.assertIn("ubuntu:22.04", bases["linux-arm64"]["url"])
        self.assertIn("debian:bookworm-slim", bases["linux-armhf"]["url"])

    def test_redaction_on_failure(self):
        # A failing tool that echoes GITHUB_TOKEN must have it scrubbed by
        # publish.sh's redacting wrapper, including every line of the secret.
        token = "fake-test-secret-first\nfake-test-secret-second"
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            stage_root = container_stage_root(root)
            env = toolchain_env(root)
            output = root / "candidate"
            run("bash", CONTAINER / "build.sh", stage_root, output, env=env, check=False)
            # Replace the fake docker with one that dumps the token then fails.
            docker = root / "bin" / "docker"
            docker.write_text("#!/usr/bin/env bash\n"
                              "if [[ \"$1\" == pull ]]; then exit 0; fi\n"
                              "while IFS= read -r line; do printf 'line=%s\\n' \"$line\" >&2; done\n"
                              "exit 1\n")
            docker.chmod(0o755)
            env["GITHUB_TOKEN"] = token
            result = run("bash", CONTAINER / "publish.sh", output, "stable",
                         workspace_version(), env=env, check=False)
            self.assertNotEqual(result.returncode, 0)
            for line in token.splitlines():
                self.assertNotIn(line, result.stdout + result.stderr)
            self.assertGreaterEqual(result.stderr.count("[redacted]"), 2)

    def test_publish_rejects_archive_from_wrong_platform(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            output, env = self.build_candidate(root)
            (output / "images/amd64.tar").write_bytes((output / "images/arm64.tar").read_bytes())
            result = self.publish(output, env)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("does not match platform", result.stderr)
            self.assertNotIn("push ", (root / "state/tags.log").read_text())
            self.assertEqual(create_tags(root), set())

    def test_workflow_publish_output_and_redaction(self):
        publish = WORKFLOW.read_text().split("        id: publish\n", 1)[1]
        run_block = publish.split("        run: |\n", 1)[1]
        run_block = re.split(r"\n(?:      - |  [\w-]+:\n)", run_block, maxsplit=1)[0]
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            stub = root / "packaging/container/publish.sh"
            stub.parent.mkdir(parents=True)
            stub.write_text(r"""#!/bin/bash
while IFS= read -r line; do
    printf 'line=%s\n' "$line" >&2
done <<< "$GITHUB_TOKEN"
exit "$STUB_STATUS"
""")
            stub.chmod(0o755)
            output = root / "output"
            secret = "fake-first\nfake-second"
            version = workspace_version()
            (root / "candidate").mkdir()
            (root / "candidate/result.json").write_text(json.dumps({"version": version}))
            for tag, status, expected in ((f"v{version}", "0", "true"),
                                          (f"beta-v{version}-rc.1", "1", "false"),
                                          ("unexpected-tag", "0", "false")):
                with self.subTest(tag=tag, status=status):
                    output.write_text("")
                    env = dict(os.environ, TAG=tag, VERSION=version, STUB_STATUS=status,
                               GITHUB_OUTPUT=str(output), GITHUB_TOKEN=secret)
                    result = subprocess.run(
                        ["bash", "-c", textwrap.dedent(run_block)], cwd=root, env=env,
                        text=True, capture_output=True,
                    )
                    self.assertEqual(result.returncode, int(expected == "false"), result.stderr)
                    self.assertEqual(output.read_text(), f"published={expected}\n")
                    for line in secret.splitlines():
                        self.assertNotIn(line, result.stdout + result.stderr)

    def test_publish_sidecar(self):
        version = workspace_version()
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            stage_root = container_stage_root(root)
            env = toolchain_env(root)
            output = root / "candidate"
            run("bash", CONTAINER / "build.sh", stage_root, output, env=env, check=False)
            result = run("bash", CONTAINER / "publish.sh", output, "stable", version,
                         env=env, check=False)
            self.assertEqual(result.returncode, 0, result.stderr)
            sidecar = json.loads(Path(str(output) + ".result.json").read_text())
            self.assertIs(sidecar["published"], True)
            self.assertEqual(sidecar["format"], "container")
            self.assertEqual(set(sidecar["tags"]), {version, "latest"})
            # The publish sidecar is a sibling of the candidate; build.sh's own
            # result.json inside the candidate is left untouched.
            self.assertTrue((output / "result.json").exists())


if __name__ == "__main__":
    unittest.main()
