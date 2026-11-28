# AirScout PlaneServer — headless container.
#
# Built per architecture by build.sh, which passes the digest-pinned base image
# (BASE_IMAGE), the distro-correct runtime loader packages (RUNTIME_PACKAGES) and
# the source URL for the OCI labels. The image runs a single non-root user
# (UID/GID 9872), mounts /config and /data for persistent state, and listens on
# 9872/tcp on all interfaces (the PlaneServer default port and bind address).
#
# The build context is the per-arch stage directory (bin/airscout-planeserver,
# README.md, LICENSE, NOTICE); nothing else is copied into the image.

# Global build args are visible to FROM but must be re-declared after FROM to be
# usable in RUN/COPY/LABEL instructions.
ARG BASE_IMAGE
FROM ${BASE_IMAGE}

ARG RUNTIME_PACKAGES

LABEL org.opencontainers.image.title="AirScout PlaneServer" \
      org.opencontainers.image.description="Aircraft tracking and aircraft scatter prediction for amateur radio (headless server)." \
      org.opencontainers.image.vendor="HamRadioOnline" \
      org.opencontainers.image.source="https://github.com/HamRadioOnline/AirScout-RS" \
      org.opencontainers.image.licenses="GPL-3.0-only"

# GTK/appindicator/xdo/ALSA/xkbcommon/wayland loader libraries the binary links
# even when headless, plus CA roots for TLS. A single layer keeps the runtime
# surface small; the package lists are dropped so no apt metadata ships.
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        ${RUNTIME_PACKAGES} \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy only the PlaneServer binary and the legal files.
COPY bin/airscout-planeserver /usr/local/bin/airscout-planeserver
COPY README.md LICENSE NOTICE /usr/share/doc/airscout-planeserver/

# Fixed non-root UID/GID so the /config and /data volumes are writable by the
# service user regardless of the host's UID/GID allocation.
RUN groupadd --gid 9872 airscout && \
    useradd --uid 9872 --gid 9872 --system --no-create-home \
        --shell /usr/sbin/nologin airscout && \
    mkdir -p /config /data && \
    chown -R 9872:9872 /config /data

VOLUME ["/config", "/data"]
EXPOSE 9872/tcp

USER 9872:9872

ENTRYPOINT ["/usr/local/bin/airscout-planeserver", "--headless", "--config", "/config/planeserver.toml", "--data", "/data"]
