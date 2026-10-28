# check=skip=InvalidDefaultArgInFrom
# test-linux-repository.sh's images for zypper: a fresh openSUSE Tumbleweed, named by BASE, that
# serves the trees of the TREES image on 127.0.0.1 inside each step and sets the repository up
# from them as the docs set up packages.usepitboard.com, with only its address changed, as root
# while the image builds. SETUP names the setup file of the suite `now` holds.
#   installed  sets up `now` and installs pitboard-app, and libzypp's plugin stores the catalog
#   refused    sets up `now` and installs pitboard-app, then keeps it from `forged`, a later
#              release whose indices another key signed
#   followed   sets up the beta channel from `beta`, installs pitboard-app, then takes `final`,
#              which the key with its subkey extended signs
ARG TREES
ARG BASE

FROM ${TREES} AS trees

FROM ${BASE} AS system
# curl to wait for the server and python3 to serve the trees; libzypp's appdata plugin, which
# stores the catalog zypper fetches, and AppStream, which reads it, as a desktop has them; gpg to
# read the key rpm keeps.
RUN zypper --non-interactive install --no-recommends curl python3 libzypp-plugin-appdata \
      AppStream gpg2 \
    && zypper clean --all
COPY --from=trees /trees/ /srv/
ARG SETUP
ENV SETUP=${SETUP} SERVE="python3 -m http.server --bind 127.0.0.1 --directory /srv/current 8080" \
    WAIT="curl --retry 30 --retry-connrefused --retry-delay 1 -fsS -o /dev/null http://127.0.0.1:8080/"

FROM system AS installed
RUN ln -s now /srv/current && { $SERVE >/dev/null 2>&1 & } && $WAIT \
    && zypper --non-interactive addrepo --refresh "http://127.0.0.1:8080/$SETUP.repo" \
    && zypper --non-interactive --gpg-auto-import-keys refresh \
    && zypper --non-interactive install --no-recommends pitboard-app

FROM system AS refused
RUN ln -s now /srv/current && { $SERVE >/dev/null 2>&1 & } && $WAIT \
    && zypper --non-interactive addrepo --refresh "http://127.0.0.1:8080/$SETUP.repo" \
    && zypper --non-interactive --gpg-auto-import-keys refresh \
    && zypper --non-interactive install --no-recommends pitboard-app && ln -sfn forged /srv/current \
    && { zypper --non-interactive refresh; \
      echo "zypper refresh exited $? on indices another key signed"; } \
    && { zypper --non-interactive update pitboard pitboard-app; echo "zypper update exited $?"; }

FROM system AS followed
RUN ln -s beta /srv/current && { $SERVE >/dev/null 2>&1 & } && $WAIT \
    && zypper --non-interactive addrepo --refresh http://127.0.0.1:8080/pitboard-beta.repo \
    && zypper --non-interactive --gpg-auto-import-keys refresh \
    && zypper --non-interactive install --no-recommends pitboard-app && ln -sfn final /srv/current \
    && zypper --non-interactive refresh && zypper --non-interactive update pitboard pitboard-app
