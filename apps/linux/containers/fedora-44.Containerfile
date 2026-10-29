# The Linux app's checks on Fedora 44, as apps/linux/scripts/in-container.sh runs them, with
# apps/linux/scripts as the build context.
FROM docker.io/library/fedora:44@sha256:43b29f65a41eb9c35e1cd5323e3bdf3b655c2357a9f4f1ff2f9c2798e5045d80
COPY install-deps.sh /tmp/install-deps.sh
RUN sh /tmp/install-deps.sh fedora-44 && rm /tmp/install-deps.sh && dnf clean all
ENV RUSTUP_HOME=/opt/rustup CARGO_HOME=/opt/cargo PATH=/opt/cargo/bin:$PATH
# rust-toolchain.toml's channel and components.
RUN rustup toolchain install stable --profile minimal --component rustfmt,clippy
# dbus-daemon refuses a uid the password database lacks.
RUN useradd --uid 1000 --user-group --no-create-home --home-dir /nonexistent \
      --shell /usr/sbin/nologin pitboard \
    && install -d -o 1000 -g 1000 /target
USER 1000:1000
