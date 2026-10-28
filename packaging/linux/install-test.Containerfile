# check=skip=InvalidDefaultArgInFrom
# test-linux-packages.sh's images: a fresh Debian or Ubuntu, named by BASE with no default since
# the script names each by digest, that installs a release's Debian packages from the build
# context with apt, as root while the image builds. `removed` purges them again.
ARG BASE
FROM ${BASE} AS installed
ENV DEBIAN_FRONTEND=noninteractive
# The validators the checks run on the installed files.
RUN apt-get install --update -y --no-install-recommends desktop-file-utils appstream \
    && rm -rf /var/lib/apt/lists/*
COPY *.deb /packages/
RUN apt-get install --update -y --no-install-recommends /packages/*.deb \
    && rm -rf /var/lib/apt/lists/*

FROM installed AS removed
RUN apt-get purge -y pitboard pitboard-app pitboard-archive-keyring
