# check=skip=InvalidDefaultArgInFrom
# test-linux-packages.sh's images for the RPM packages on Fedora: a fresh Fedora, named by BASE
# with no default since the script names each by digest, with the validators the checks run on
# the installed files. `installed` installs the packages from the build context's packages folder
# with dnf, as root while the image builds, and `removed` removes them again.
ARG BASE
FROM ${BASE} AS system
# dbus-daemon brings dbus-common, which holds the D-Bus folders on a system with a bus. Without it
# Fedora 44's glib-networking, which GTK brings through tinysparql and libsoup3, puts a file in
# dbus-1/services and holds no folder, so dnf, removing it with the packages, leaves that folder.
RUN dnf install -y --setopt=install_weak_deps=False desktop-file-utils appstream dbus-daemon \
    && dnf clean all

FROM system AS installed
COPY packages/ /packages/
RUN dnf install -y --setopt=install_weak_deps=False /packages/*.rpm && dnf clean all

FROM installed AS removed
RUN dnf remove -y pitboard pitboard-app && dnf clean all
