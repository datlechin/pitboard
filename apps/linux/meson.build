# The Linux app's release: Cargo builds the program, and Meson installs it with its launcher,
# D-Bus service, metainfo, icons, search provider and KRunner runner, and with the Plasma widget
# unless plasma_widget is off. Nothing else installs them: a build Cargo makes alone is the debug
# build (src/config.rs).
project('pitboard-linux', license: 'Apache-2.0', meson_version: '>= 1.7.0')

fs = import('fs')

app_id = 'com.usepitboard.Pitboard'
bindir = get_option('prefix') / get_option('bindir')
datadir = get_option('prefix') / get_option('datadir')
icons = datadir / 'icons' / 'hicolor'

python = find_program('python3')
desktop_file_validate = find_program('desktop-file-validate')
appstreamcli = find_program('appstreamcli')
rsvg_convert = find_program('rsvg-convert')

custom_target(
  'pitboard-app',
  output: 'pitboard-app',
  command: [
    files('scripts/cargo-build.sh'),
    meson.project_source_root() / '..' / '..' / 'Cargo.toml',
    meson.current_build_dir() / 'target',
    '@OUTPUT@',
  ],
  env: {
    'PITBOARD_BUILD': 'release',
    'PITBOARD_BINDIR': bindir,
    'PITBOARD_PACKAGE': get_option('package'),
  },
  # Cargo knows what changed; Meson asks it every build.
  build_always_stale: true,
  build_by_default: true,
  console: true,
  install: true,
  install_dir: bindir,
)

paths = {'BINDIR': bindir}
launcher = configure_file(
  input: 'data' / app_id + '.desktop.in',
  output: app_id + '.desktop',
  configuration: paths,
  install_dir: datadir / 'applications',
)
configure_file(
  input: 'data' / app_id + '.service.in',
  output: app_id + '.service',
  configuration: paths,
  install_dir: datadir / 'dbus-1' / 'services',
)
metainfo = custom_target(
  'metainfo',
  input: ['data' / app_id + '.metainfo.xml.in', '../../CHANGELOG.md'],
  output: app_id + '.metainfo.xml',
  command: [python, files('data/metainfo-releases.py'), '@INPUT@'],
  capture: true,
  install: true,
  install_dir: datadir / 'metainfo',
)
install_data(
  'data' / app_id + '.search-provider.ini',
  install_dir: datadir / 'gnome-shell' / 'search-providers',
)
runner = files('data/krunner' / app_id + '.desktop')
install_data(runner, install_dir: datadir / 'krunner' / 'dbusplugins')

# A package of its own, since it needs Plasma 6.4, which Debian 13 does not ship.
if get_option('plasma_widget')
  install_subdir('plasmoid' / app_id, install_dir: datadir / 'plasma' / 'plasmoids')
endif

app_icon = files('data/icons/scalable/apps' / app_id + '.svg')
symbolic_icon = files('data/icons/symbolic/apps' / app_id + '-symbolic.svg')
status_icons = []
foreach level : ['plenty', 'low', 'out', 'none']
  foreach attention : ['', '-attention']
    status_icons += files(
      'data/icons/symbolic/status' / app_id + '-' + level + attention + '-symbolic.svg',
    )
  endforeach
endforeach
install_data(app_icon, install_dir: icons / 'scalable' / 'apps')
install_data(symbolic_icon, install_dir: icons / 'symbolic' / 'apps')
install_data(status_icons, install_dir: icons / 'symbolic' / 'status')

test('launcher', desktop_file_validate, args: [launcher])
test('runner', desktop_file_validate, args: [runner])
test('metainfo', appstreamcli, args: ['validate', '--no-net', metainfo])
devel_icon = files('data/icons/scalable/apps' / app_id + '.Devel.svg')
# rsvg-convert writes only a regular file.
foreach icon : app_icon + devel_icon + symbolic_icon + status_icons
  drawn = meson.current_build_dir() / fs.stem(icon) + '.png'
  test(fs.name(icon), rsvg_convert, args: ['--output', drawn, icon], suite: 'icons')
endforeach
