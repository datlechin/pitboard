# The Linux app's checks on Debian 13, its floor, as apps/linux/scripts/in-container.sh runs
# them, with apps/linux/scripts as the build context.
FROM docker.io/library/debian:13@sha256:8c4733a92aacd0bcff3779eadc35a4a5147f36cb20a5fcb56504cbc740212259
COPY install-deps.sh /tmp/install-deps.sh
RUN sh /tmp/install-deps.sh debian-13 && rm -rf /tmp/install-deps.sh /var/lib/apt/lists/*
ENV RUSTUP_HOME=/opt/rustup CARGO_HOME=/opt/cargo PATH=/opt/cargo/bin:$PATH
# rust-toolchain.toml's channel and components, and the app's floor, which `check.sh floor`
# checks with no network.
RUN rustup toolchain install stable --profile minimal --component rustfmt,clippy \
    && rustup toolchain install 1.92 --profile minimal
# dbus-daemon refuses a uid the password database lacks.
RUN useradd --uid 1000 --user-group --no-create-home --home-dir /nonexistent \
      --shell /usr/sbin/nologin pitboard \
    && install -d -o 1000 -g 1000 /target
USER 1000:1000
