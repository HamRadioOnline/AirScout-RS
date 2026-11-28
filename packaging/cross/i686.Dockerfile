# Debian provides the full i386 GTK and tray development dependencies.
FROM docker.io/library/debian:bookworm-slim@sha256:311f7a192fd9a92b165a86b32fa3aae9005b11849bfb1db2d4fd784559ee464f
ARG DEBIAN_FRONTEND=noninteractive
RUN dpkg --add-architecture i386 \
    && apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates gcc g++ pkg-config \
        gcc-i686-linux-gnu g++-i686-linux-gnu libc6-dev-i386-cross \
        libgtk-3-dev:i386 libasound2-dev:i386 libayatana-appindicator3-dev:i386 libxdo-dev:i386 \
    && rm -rf /var/lib/apt/lists/*

ENV CARGO_TARGET_I686_UNKNOWN_LINUX_GNU_LINKER=i686-linux-gnu-gcc \
    CARGO_TARGET_I686_UNKNOWN_LINUX_GNU_RUSTFLAGS="-C link-arg=-Wl,-rpath-link,/usr/lib/i386-linux-gnu" \
    AR_i686_unknown_linux_gnu=i686-linux-gnu-ar \
    CC_i686_unknown_linux_gnu=i686-linux-gnu-gcc \
    CXX_i686_unknown_linux_gnu=i686-linux-gnu-g++ \
    PKG_CONFIG_ALLOW_CROSS=1 \
    PKG_CONFIG_LIBDIR_i686_unknown_linux_gnu=/usr/lib/i386-linux-gnu/pkgconfig:/usr/share/pkgconfig
