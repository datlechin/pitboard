# check=skip=InvalidDefaultArgInFrom
# test-linux-repository.sh's repositories, built from the release's packages in the build
# context's packages folder by the repository's own scripts, as uid 1000, with keys made here.
# UBUNTU and FEDORA have no default since the script names each by digest, and SUITE, BETA and
# FINAL are the suite of the packages given and the test's releases' versions. `tools` is Ubuntu
# 26.04 with install-deps.sh's repository set, `signer` Fedora 43 with its own, and `trees` holds
# the trees repository-test.sh builds, in /trees, for the clients' images.
ARG UBUNTU
ARG FEDORA

FROM ${UBUNTU} AS tools
COPY repo/apps/linux/scripts/install-deps.sh /tmp/install-deps.sh
RUN sh /tmp/install-deps.sh ubuntu-26.04 repository && rm -rf /tmp/install-deps.sh /var/lib/apt/lists/*
# uid 1000 is the image's own user, ubuntu.
USER 1000:1000
ENV HOME=/tmp/home XDG_CONFIG_HOME=/tmp/home/.config XDG_DATA_HOME=/tmp/home/.local/share \
    XDG_CACHE_HOME=/tmp/home/.cache XDG_STATE_HOME=/tmp/home/.local/state

FROM ${FEDORA} AS signer
COPY repo/apps/linux/scripts/install-deps.sh /tmp/install-deps.sh
# rpmrebuild makes the test's releases from the packages given.
RUN sh /tmp/install-deps.sh fedora-43 repository \
    && dnf install -y --repo fedora --repo updates --setopt=install_weak_deps=False rpmrebuild \
    && dnf clean all && rm /tmp/install-deps.sh
RUN useradd --uid 1000 --user-group --no-create-home --home-dir /tmp/home pitboard
USER 1000:1000
ENV HOME=/tmp/home XDG_CONFIG_HOME=/tmp/home/.config XDG_DATA_HOME=/tmp/home/.local/share \
    XDG_CACHE_HOME=/tmp/home/.cache XDG_STATE_HOME=/tmp/home/.local/state

FROM tools AS keys
COPY repo/ /src/
RUN mkdir -p "$HOME" && sh /src/packaging/linux/repository-test.sh keys /tmp/keys

FROM signer AS rpms
ARG BETA
ARG FINAL
COPY repo/ /src/
COPY packages/ /tmp/packages/
COPY --from=keys /tmp/keys/ /tmp/keys/
RUN mkdir -p "$HOME" && sh /src/packaging/linux/repository-test.sh rpms /tmp/keys /tmp/packages \
    "$BETA" "$FINAL" /tmp/releases

FROM tools AS built
ARG SUITE
ARG BETA
ARG FINAL
COPY repo/ /src/
COPY packages/ /tmp/packages/
COPY --from=keys /tmp/keys/ /tmp/keys/
COPY --from=rpms --chown=1000:1000 /tmp/releases/ /tmp/releases/
RUN mkdir -p "$HOME" && sh /src/packaging/linux/repository-test.sh trees /tmp/keys /tmp/packages \
    /tmp/releases "$SUITE" "$BETA" "$FINAL" /tmp/trees

FROM scratch AS trees
COPY --from=built /tmp/trees/ /trees/
