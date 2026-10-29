# The Linux app's checks on Ubuntu 26.04, as apps/linux/scripts/in-container.sh runs them, with
# apps/linux/scripts as the build context.
FROM docker.io/library/ubuntu:26.04@sha256:0e73b71479e7bc5f006bca872e25c3cd680dc052a0116f90d63ae363537db0dd
COPY install-deps.sh /tmp/install-deps.sh
RUN sh /tmp/install-deps.sh ubuntu-26.04 && rm -rf /tmp/install-deps.sh /var/lib/apt/lists/*
ENV RUSTUP_HOME=/opt/rustup CARGO_HOME=/opt/cargo PATH=/opt/cargo/bin:$PATH
# rust-toolchain.toml's channel and components.
RUN rustup toolchain install stable --profile minimal --component rustfmt,clippy
# uid 1000 is the image's own user, ubuntu.
RUN install -d -o 1000 -g 1000 /target
USER 1000:1000
