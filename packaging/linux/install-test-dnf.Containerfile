# check=skip=InvalidDefaultArgInFrom
# test-linux-packages.sh's images for the RPM packages on Fedora: a fresh Fedora, named by BASE
# with no default since the script names each by digest, with the validators the checks run on
# the installed files, and PLASMA, the system's Plasma where the test installs the widget, as a
# Plasma desktop has it. `installed` installs the packages from the build context's packages
# folder with dnf, as root while the image builds, and `removed` removes PACKAGES, the ones it
# installed, again.
ARG BASE
FROM ${BASE} AS system
ARG PLASMA
# dbus-daemon brings dbus-common, which holds the D-Bus folders on a system with a bus. Without it
# Fedora 44's glib-networking, which GTK brings through tinysparql and libsoup3, puts a file in
# dbus-1/services and holds no folder, so dnf, removing it with the packages, leaves that folder.
RUN dnf install -y --setopt=install_weak_deps=False desktop-file-utils appstream dbus-daemon \
    $PLASMA && dnf clean all

FROM system AS installed
COPY packages/ /packages/
RUN dnf install -y --setopt=install_weak_deps=False /packages/*.rpm && dnf clean all

FROM installed AS removed
ARG PACKAGES
RUN dnf remove -y $PACKAGES && dnf clean all
