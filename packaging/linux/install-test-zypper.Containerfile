# check=skip=InvalidDefaultArgInFrom
# test-linux-packages.sh's images for the RPM packages on openSUSE: a fresh Tumbleweed, named by
# BASE with no default since the script names each by digest, with the validators the checks run
# on the installed files. `installed` installs the packages from the build context's packages
# folder with zypper, as root while the image builds, and `removed` removes them again. The
# packages are a build's, which only a release signs.
ARG BASE
FROM ${BASE} AS system
RUN zypper --non-interactive install --no-recommends desktop-file-utils AppStream \
    && zypper clean --all

FROM system AS installed
COPY packages/ /packages/
RUN zypper --non-interactive install --no-recommends --allow-unsigned-rpm /packages/*.rpm \
    && zypper clean --all

FROM installed AS removed
RUN zypper --non-interactive remove pitboard pitboard-app && zypper clean --all
