# check=skip=InvalidDefaultArgInFrom
# test-linux-packages.sh's images for the Debian packages: a fresh Debian or Ubuntu, named by
# BASE with no default since the script names each by digest, with the validators the checks run
# on the installed files, and PLASMA, the system's Plasma where the test installs the widget, as
# a Plasma desktop has it. `installed` installs the packages from the build context's packages
# folder with apt, as root while the image builds, and `removed` purges PACKAGES, the ones it
# installed, again.
ARG BASE
FROM ${BASE} AS system
ARG PLASMA
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get install --update -y --no-install-recommends desktop-file-utils appstream $PLASMA \
    && rm -rf /var/lib/apt/lists/*

FROM system AS installed
COPY packages/ /packages/
RUN apt-get install --update -y --no-install-recommends /packages/*.deb \
    && rm -rf /var/lib/apt/lists/*

FROM installed AS removed
ARG PACKAGES
RUN apt-get purge -y $PACKAGES
