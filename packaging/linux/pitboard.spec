# The command line and the app. build-linux-packages.sh defines pitboard_version, the release's
# own version, pitboard_rpm_version and pitboard_rpm_release, as linux-versions.sh prints them,
# and pitboard_date, the commit's day, for the changelog. Source0 is the commit as git archive
# writes it, and Source1 the release's command line tarball.

# Cargo's release profile strips both programs, so there is no debug information to split off.
%global debug_package %{nil}
# Fedora's Rust flags keep debug information for a debuginfo package; without them Cargo builds
# the app as the workspace's release profile says, as every other release of it is built.
%undefine build_rustflags
# The command line is the release tarball's binary byte for byte, as every other way to install
# it has it, so nothing takes its .comment section out.
%global __brp_strip_comment_note %{nil}
%global _vpath_srcdir apps/linux
%global command_line pitboard-v%{pitboard_version}-%{_arch}-unknown-linux-musl

Name: pitboard
Version: %{pitboard_rpm_version}
Release: %{pitboard_rpm_release}
Summary: Command line that switches AI coding tools between your accounts
# The programs link the Rust crates Cargo.lock names, each under its own license. The bill of
# materials on each release lists them.
License: Apache-2.0
URL: https://usepitboard.com
Source0: pitboard.tar
Source1: %{command_line}.tar.gz
ExclusiveArch: x86_64 aarch64

# No rust or cargo: rust-toolchain.toml's stable comes from rustup, as on every system that
# builds Pitboard.
BuildRequires: gcc
BuildRequires: meson
BuildRequires: pkgconfig(gtk4) >= 4.18
BuildRequires: pkgconfig(libadwaita-1) >= 1.7
BuildRequires: pkgconfig(glib-2.0) >= 2.84
BuildRequires: desktop-file-utils
BuildRequires: appstream
BuildRequires: librsvg2-tools
BuildRequires: python3

%description
Pitboard switches Claude Code and OpenAI's Codex CLI between your own accounts
without signing you out, and shows what each account has left of its limits.

This package holds the pitboard command, its manual page and its bash, zsh and
fish completions.

%package -n pitboard-app
Summary: App that switches AI coding tools between your accounts
# Renewal and the watch service run the pitboard beside the app, with this version's options.
Requires: pitboard = %{version}-%{release}
# The floor. rpm's ELF generator requires GTK, libadwaita and GLib by soname alone, which any 4.x,
# 1.x and 2.x meets, and Fedora's gtk4, libadwaita and glib2 and openSUSE's libgtk-4-1,
# libadwaita-1-0 and libglib-2_0-0 each provide these names at their own version.
Requires: gtk4 >= 4.18
Requires: libadwaita >= 1.7
Requires: glib2 >= 2.84
# The theme the app's icons are in, which holds its folders but symbolic/status.
Requires: hicolor-icon-theme

%description -n pitboard-app
Pitboard switches Claude Code and OpenAI's Codex CLI between your own accounts
without signing you out, and shows what each account has left of its limits.

This package holds the desktop app.

%prep
%autosetup -c -a 1

%build
# The Plasma widget, which needs Plasma 6.4, is not part of the app's package.
%meson -Dpackage=rpm -Dplasma_widget=false
%meson_build

%install
%meson_install
install -D -m 0755 %{command_line}/pitboard %{buildroot}%{_bindir}/pitboard
install -D -m 0644 %{command_line}/pitboard.1 %{buildroot}%{_mandir}/man1/pitboard.1
install -D -m 0644 %{command_line}/completions/pitboard.bash \
  %{buildroot}%{bash_completions_dir}/pitboard
install -D -m 0644 %{command_line}/completions/pitboard.zsh \
  %{buildroot}%{zsh_completions_dir}/_pitboard
install -D -m 0644 %{command_line}/completions/pitboard.fish \
  %{buildroot}%{fish_completions_dir}/pitboard.fish

%check
%meson_test

# A folder no package the packages require is sure to hold is the packages' too, so that
# removing them takes it away. Fedora's filesystem holds zsh's and fish's, Tumbleweed's neither.
%files
%license LICENSE NOTICE
%{_bindir}/pitboard
%{_mandir}/man1/pitboard.1*
%{bash_completions_dir}/pitboard
%dir %{_datadir}/zsh
%dir %{zsh_completions_dir}
%{zsh_completions_dir}/_pitboard
%dir %{_datadir}/fish
%dir %{fish_completions_dir}
%{fish_completions_dir}/pitboard.fish

%files -n pitboard-app
%license LICENSE NOTICE
%{_bindir}/pitboard-app
%{_mandir}/man1/pitboard-app.1*
%{_datadir}/applications/com.usepitboard.Pitboard.desktop
%dir %{_datadir}/dbus-1
%dir %{_datadir}/dbus-1/services
%{_datadir}/dbus-1/services/com.usepitboard.Pitboard.service
%dir %{_datadir}/gnome-shell
%dir %{_datadir}/gnome-shell/search-providers
%{_datadir}/gnome-shell/search-providers/com.usepitboard.Pitboard.search-provider.ini
%{_datadir}/icons/hicolor/scalable/apps/com.usepitboard.Pitboard.svg
%{_datadir}/icons/hicolor/symbolic/apps/com.usepitboard.Pitboard-symbolic.svg
%dir %{_datadir}/icons/hicolor/symbolic/status
%{_datadir}/icons/hicolor/symbolic/status/com.usepitboard.Pitboard-*-symbolic.svg
%dir %{_datadir}/krunner
%dir %{_datadir}/krunner/dbusplugins
%{_datadir}/krunner/dbusplugins/com.usepitboard.Pitboard.desktop
%{_datadir}/metainfo/com.usepitboard.Pitboard.metainfo.xml

%changelog
* %{pitboard_date} Ngo Quoc Dat <datlechin@gmail.com> - %{version}-%{release}
- Pitboard %{pitboard_version}.
