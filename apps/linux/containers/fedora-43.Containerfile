# The Linux app's RPM packages' build on Fedora 43, the oldest Fedora of its floor, as
# apps/linux/scripts/in-container.sh runs it, with apps/linux/scripts as the build context.
FROM docker.io/library/fedora:43@sha256:a651ddf48ea28a06ed4e1e6519f51c9f47e7a5a138722ade87369b8fbb7e5b42
COPY install-deps.sh /tmp/install-deps.sh
RUN sh /tmp/install-deps.sh fedora-43 packaging && rm /tmp/install-deps.sh && dnf clean all
ENV RUSTUP_HOME=/opt/rustup CARGO_HOME=/opt/cargo PATH=/opt/cargo/bin:$PATH
# rust-toolchain.toml's channel and components.
RUN rustup toolchain install stable --profile minimal --component rustfmt,clippy
# dbus-daemon refuses a uid the password database lacks.
RUN useradd --uid 1000 --user-group --no-create-home --home-dir /nonexistent \
      --shell /usr/sbin/nologin pitboard \
    && install -d -o 1000 -g 1000 /target
USER 1000:1000
