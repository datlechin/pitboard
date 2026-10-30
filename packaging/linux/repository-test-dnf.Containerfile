# check=skip=InvalidDefaultArgInFrom
# test-linux-repository.sh's images for dnf: a fresh Fedora, named by BASE, that serves the trees
# of the TREES image on 127.0.0.1 inside each step and sets the repository up from them as the
# docs set up packages.usepitboard.com, with only its address changed, as root while the image
# builds. SETUP names the setup file of the suite `now` holds.
#   installed  sets up `now` and installs pitboard-app, which stores the catalog
#   refused    sets up `now` and installs pitboard-app, then keeps it from `forged`, a later
#              release whose indices another key signed
#   followed   sets up the beta channel from `beta`, installs pitboard-app, then takes `final`,
#              which the key with its subkey extended signs
ARG TREES
ARG BASE

FROM ${TREES} AS trees

FROM ${BASE} AS system
# dnf's config-manager, curl to wait for the server and python3 to serve the trees;
# libdnf5-plugin-appstream, which stores the catalog dnf fetches, and appstream, which reads it;
# gpg to read the key rpm keeps.
RUN dnf install -y --setopt=install_weak_deps=False dnf5-plugins curl python3 \
      libdnf5-plugin-appstream appstream gnupg2 \
    && dnf clean all
COPY --from=trees /trees/ /srv/
ARG SETUP
ENV SETUP=${SETUP} SERVE="python3 -m http.server --bind 127.0.0.1 --directory /srv/current 8080" \
    WAIT="curl --retry 30 --retry-connrefused --retry-delay 1 -fsS -o /dev/null http://127.0.0.1:8080/"

FROM system AS installed
RUN ln -s now /srv/current && { $SERVE >/dev/null 2>&1 & } && $WAIT \
    && dnf config-manager addrepo --from-repofile="http://127.0.0.1:8080/$SETUP.repo" \
    && dnf install -y --setopt=optional_metadata_types=appstream pitboard-app

FROM system AS refused
RUN ln -s now /srv/current && { $SERVE >/dev/null 2>&1 & } && $WAIT \
    && dnf config-manager addrepo --from-repofile="http://127.0.0.1:8080/$SETUP.repo" \
    && dnf install -y pitboard-app && ln -sfn forged /srv/current \
    && { dnf makecache --refresh; echo "dnf makecache exited $? on indices another key signed"; } \
    && { dnf upgrade -y pitboard pitboard-app; echo "dnf upgrade exited $?"; }

FROM system AS followed
RUN ln -s beta /srv/current && { $SERVE >/dev/null 2>&1 & } && $WAIT \
    && dnf config-manager addrepo --from-repofile=http://127.0.0.1:8080/pitboard-beta.repo \
    && dnf install -y pitboard-app && ln -sfn final /srv/current \
    && dnf upgrade -y --refresh pitboard pitboard-app
