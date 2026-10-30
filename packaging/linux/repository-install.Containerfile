# check=skip=InvalidDefaultArgInFrom
# release.yml's linux-repository-install images: a fresh system, named by BASE, that sets up the
# channel of packages.usepitboard.com whose setup file SETUP names, pitboard or pitboard-beta, as
# the docs say, and installs pitboard-app at VERSION, the release's package version, as root while
# the image builds. Each waits up to two minutes for the channel to serve that version, since a
# cache in front of the bucket may hold the indices before it. One target for each package manager.
ARG BASE

FROM ${BASE} AS apt
ARG SETUP
ARG VERSION
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get install --update -y --no-install-recommends ca-certificates curl \
    && curl -fsSL -o /usr/share/keyrings/pitboard-archive-keyring.pgp \
      https://packages.usepitboard.com/pitboard-archive-keyring.pgp \
    && curl -fsSL -o "/etc/apt/sources.list.d/$SETUP.sources" \
      "https://packages.usepitboard.com/$SETUP.sources" \
    && curl -fsSL -o /etc/apt/preferences.d/pitboard.pref \
      https://packages.usepitboard.com/pitboard.pref \
    && for attempt in 1 2 3 4 5 6 7 8 9 10 11 12; do \
      apt-get update --error-on=any \
        && [ "$(apt-cache policy pitboard-app | sed -n 's/^ *Candidate: //p')" = "$VERSION" ] \
        && break; \
      echo "packages.usepitboard.com does not offer pitboard-app $VERSION yet (attempt $attempt)"; \
      sleep 10; \
    done \
    && apt-get install -y "pitboard-app=$VERSION"

FROM ${BASE} AS dnf
ARG SETUP
ARG VERSION
# dnf's config-manager.
RUN dnf install -y --setopt=install_weak_deps=False dnf5-plugins \
    && dnf config-manager addrepo \
      "--from-repofile=https://packages.usepitboard.com/$SETUP.repo" \
    && for attempt in 1 2 3 4 5 6 7 8 9 10 11 12; do \
      [ "$(dnf repoquery -y --refresh --quiet --repo "$SETUP" --queryformat '%{version}\n' \
        pitboard-app)" = "$VERSION" ] && break; \
      echo "packages.usepitboard.com does not offer pitboard-app $VERSION yet (attempt $attempt)"; \
      sleep 10; \
    done \
    && dnf install -y "pitboard-app-$VERSION"

FROM ${BASE} AS zypper
ARG SETUP
ARG VERSION
RUN zypper --non-interactive addrepo --refresh "https://packages.usepitboard.com/$SETUP.repo" \
    && for attempt in 1 2 3 4 5 6 7 8 9 10 11 12; do \
      zypper --non-interactive --gpg-auto-import-keys refresh "$SETUP" \
        && [ "$(zypper --non-interactive info --repo "$SETUP" pitboard-app \
          | sed -n 's/^Version *: //p')" = "$VERSION-1" ] \
        && break; \
      echo "packages.usepitboard.com does not offer pitboard-app $VERSION yet (attempt $attempt)"; \
      sleep 10; \
    done \
    && zypper --non-interactive install --no-recommends "pitboard-app = $VERSION-1"
