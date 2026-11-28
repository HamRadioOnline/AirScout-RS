"""Read and verify the image descriptor and config in a single-platform OCI archive."""

import hashlib
import json
import re
import sys
import tarfile

OCI_MANIFEST = "application/vnd.oci.image.manifest.v1+json"


def read_member(archive, name):
    member = archive.getmember(name)
    if not member.isfile():
        raise ValueError(f"expected a regular archive member at {name}")
    with archive.extractfile(member) as file:
        return file.read()


def descriptor(archive_path, platform):
    with tarfile.open(archive_path, "r:") as archive:
        def blob(entry):
            digest = entry["digest"]
            if not re.fullmatch(r"sha256:[a-f0-9]{64}", digest):
                raise ValueError(f"invalid OCI digest {digest!r}")
            data = read_member(archive, "blobs/" + digest.replace(":", "/", 1))
            if len(data) != entry["size"] or digest != "sha256:" + hashlib.sha256(data).hexdigest():
                raise ValueError(f"OCI blob does not match descriptor {digest}")
            return json.loads(data)

        layout = json.loads(read_member(archive, "oci-layout"))
        if layout["imageLayoutVersion"] != "1.0.0":
            raise ValueError("unsupported OCI layout version")
        image, = json.loads(read_member(archive, "index.json"))["manifests"]
        manifest = blob(image)
        if image["mediaType"] != OCI_MANIFEST or manifest["mediaType"] != OCI_MANIFEST:
            raise ValueError("expected one OCI image manifest")
        config = manifest["config"]
        settings = blob(config)
        expected_os, expected_arch, *variant = platform.split("/")
        if (settings["os"], settings["architecture"]) != (expected_os, expected_arch):
            raise ValueError(f"OCI image config does not match platform {platform}")
        if variant and settings.get("variant", variant[0]) != variant[0]:
            raise ValueError(f"OCI image config does not match variant for {platform}")
        return image["digest"], image["mediaType"], image["size"], config["digest"]


def docker_config_digest(archive_path, expected_tag):
    with tarfile.open(archive_path, "r:") as archive:
        image, = json.loads(read_member(archive, "manifest.json"))
        if image["RepoTags"] != [expected_tag]:
            raise ValueError("Docker archive does not contain the expected smoke-test tag")
        config = read_member(archive, image["Config"])
        return "sha256:" + hashlib.sha256(config).hexdigest()


if __name__ == "__main__":
    archive_type, path, target = sys.argv[1:]
    try:
        if archive_type == "oci":
            print(*descriptor(path, target), sep="\t")
        elif archive_type == "docker":
            print(docker_config_digest(path, target))
        else:
            raise ValueError(f"unknown archive type {archive_type}")
    except (KeyError, OSError, TypeError, ValueError, tarfile.TarError) as error:
        sys.exit(f"invalid {archive_type} archive {path}: {error}")
