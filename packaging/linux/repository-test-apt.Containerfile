# check=skip=InvalidDefaultArgInFrom
# test-linux-repository.sh's images for apt: a fresh Debian or Ubuntu, named by BASE, that serves
# the trees of the TREES image on 127.0.0.1 inside each step and sets the repository up from them
# as the docs set up packages.usepitboard.com, with only its address changed, as root while the
# image builds. SETUP names the setup file of the suite `now` holds.
#   installed  sets up `now` and installs pitboard-app, which brings the keyring by Recommends
#   refused    sets up `now` and installs pitboard-app, then keeps it from `forged`, a later
#              release whose indices another key signed
#   followed   sets up the beta channel from `beta`, installs pitboard-app, then takes `final`,
#              whose keyring holds the key with its subkey extended, and updates from it after
ARG TREES
ARG BASE

FROM ${TREES} AS trees

FROM ${BASE} AS system
ENV DEBIAN_FRONTEND=noninteractive
# curl for the setup files and python3 to serve the trees; appstream, whose apt configuration
# fetches the catalog, and apt-config-icons, which fetches its icons, as a desktop has them.
RUN apt-get install --update -y --no-install-recommends ca-certificates curl python3 appstream \
      apt-config-icons \
    && rm -rf /var/lib/apt/lists/*
COPY --from=trees /trees/ /srv/
ARG SETUP
ENV SETUP=${SETUP} SERVE="python3 -m http.server --bind 127.0.0.1 --directory /srv/current 8080" \
    WAIT="curl --retry 30 --retry-connrefused --retry-delay 1 -fsS -o /dev/null http://127.0.0.1:8080/"

FROM system AS installed
RUN ln -s now /srv/current && { $SERVE >/dev/null 2>&1 & } && $WAIT \
    && curl -fsSL -o /usr/share/keyrings/pitboard-archive-keyring.pgp \
      http://127.0.0.1:8080/pitboard-archive-keyring.pgp \
    && curl -fsSL -o "/etc/apt/sources.list.d/$SETUP.sources" \
      "http://127.0.0.1:8080/$SETUP.sources" \
    && curl -fsSL -o /etc/apt/preferences.d/pitboard.pref http://127.0.0.1:8080/pitboard.pref \
    && apt-get update && apt-get install -y pitboard-app

# pitboard-app without the keyring by Recommends, which holds the committed key, so that the key
# `forged` meets is the test's.
FROM system AS refused
RUN ln -s now /srv/current && { $SERVE >/dev/null 2>&1 & } && $WAIT \
    && curl -fsSL -o /usr/share/keyrings/pitboard-archive-keyring.pgp \
      http://127.0.0.1:8080/pitboard-archive-keyring.pgp \
    && curl -fsSL -o "/etc/apt/sources.list.d/$SETUP.sources" \
      "http://127.0.0.1:8080/$SETUP.sources" \
    && curl -fsSL -o /etc/apt/preferences.d/pitboard.pref http://127.0.0.1:8080/pitboard.pref \
    && apt-get update && apt-get install -y --no-install-recommends pitboard-app \
    && ln -sfn forged /srv/current \
    && { apt-get update; echo "apt-get update exited $? on indices another key signed"; } \
    && apt-get install --only-upgrade -y pitboard pitboard-app pitboard-archive-keyring

FROM system AS followed
RUN ln -s beta /srv/current && { $SERVE >/dev/null 2>&1 & } && $WAIT \
    && curl -fsSL -o /usr/share/keyrings/pitboard-archive-keyring.pgp \
      http://127.0.0.1:8080/pitboard-archive-keyring.pgp \
    && curl -fsSL -o /etc/apt/sources.list.d/pitboard-beta.sources \
      http://127.0.0.1:8080/pitboard-beta.sources \
    && curl -fsSL -o /etc/apt/preferences.d/pitboard.pref http://127.0.0.1:8080/pitboard.pref \
    && apt-get update && apt-get install -y pitboard-app && ln -sfn final /srv/current \
    && apt-get update --error-on=any \
    && apt-get install --only-upgrade -y pitboard pitboard-app pitboard-archive-keyring \
    && apt-get update --error-on=any
