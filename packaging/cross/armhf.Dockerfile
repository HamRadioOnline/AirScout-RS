# Use the same Debian release for the cross toolchain and target development libraries.
FROM docker.io/library/debian:bookworm-slim@sha256:311f7a192fd9a92b165a86b32fa3aae9005b11849bfb1db2d4fd784559ee464f
ARG DEBIAN_FRONTEND=noninteractive
RUN dpkg --add-architecture armhf \
    && apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates gcc g++ pkg-config \
        gcc-arm-linux-gnueabihf g++-arm-linux-gnueabihf libc6-dev-armhf-cross \
        libgtk-3-dev:armhf libasound2-dev:armhf libayatana-appindicator3-dev:armhf libxdo-dev:armhf \
    && rm -rf /var/lib/apt/lists/*

ENV CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABIHF_LINKER=arm-linux-gnueabihf-gcc \
    CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABIHF_RUSTFLAGS="-C link-arg=-Wl,-rpath-link,/usr/lib/arm-linux-gnueabihf" \
    AR_armv7_unknown_linux_gnueabihf=arm-linux-gnueabihf-ar \
    CC_armv7_unknown_linux_gnueabihf=arm-linux-gnueabihf-gcc \
    CXX_armv7_unknown_linux_gnueabihf=arm-linux-gnueabihf-g++ \
    PKG_CONFIG_ALLOW_CROSS=1 \
    PKG_CONFIG_LIBDIR_armv7_unknown_linux_gnueabihf=/usr/lib/arm-linux-gnueabihf/pkgconfig:/usr/share/pkgconfig
