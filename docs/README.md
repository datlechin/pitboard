# Pitboard documentation

This folder is the source of [docs.usepitboard.com](https://docs.usepitboard.com), built
with Mintlify. Pages are `.mdx` files, and `docs.json` holds the sidebar and site settings.
The writing rules are in [AGENTS.md](AGENTS.md).

Merging a change to `main` publishes the site: Mintlify builds it from this folder.

## Preview

Install the Mintlify CLI once, then run it in this folder:

```sh
npm i -g mint
mint dev
```

The site opens at `http://localhost:3000`. The first run downloads Mintlify's preview
client, about 360 MB. Restart `mint dev` after changing `docs.json`, because it misses
some changes to that file.

## Check

No CI job runs these checks, so run them in this folder before each commit. All three must
pass:

```sh
mint validate
mint broken-links --check-anchors --check-redirects
mint a11y
```

The website's checks read this folder too, and CI runs them on a change here. Its tests fail
on a link from outside `docs/` to a page or heading that is gone, and its build fails on a
link from a page here to an id usepitboard.com does not have. Run them as
[Website](../CONTRIBUTING.md#website) says.

## Where things go

- Each page goes in the folder its sidebar group uses, such as `guides/` for
  **Use Pitboard**, and in that group in `docs.json`. `index`, `quickstart`, `install`,
  `troubleshooting` and `security` sit at the top.
- A page few readers still need, such as `install/upgrade-from-0-3-0`, keeps its place in
  `docs.json` and gets `hidden: true` in its frontmatter. Mintlify then leaves it out of the
  sidebar, the search and the sitemap, and its URL keeps working.
- Images go in `images/` and are referenced as `/images/name.png`. The apps' UI tests take
  the pictures of the menu and the window. `menu.png` and `window.png` come from the macOS
  app, as [Run the debug build](../CONTRIBUTING.md#run-the-debug-build) says.
  `menu-windows.png` and `window-windows.png` come from CI's `windows-app-pictures`, as
  [The app on CI](../CONTRIBUTING.md#the-app-on-ci) says. `widget.png` is
  `macos-widget-medium-data.png` from CI's `macos-widget-pictures`, as
  [Run the debug build](../CONTRIBUTING.md#run-the-debug-build) says.
- `.mintignore` lists the files Mintlify must not publish, such as `AGENTS.md` and `facts/`,
  the measured facts the code rests on.

Pitboard's mark is the board the apps' icons draw. Nothing regenerates its copies, so change them
together:

- `apps/macos/App/Assets.xcassets`: the macOS menu bar symbol, and its variant with a badge in the
  same margins.
- `favicon.svg`, `favicon-dark.svg` and `logo/` in this folder: the site icons and header logos.
- `apps/windows/Icon/`: the drawings `make-ico.py` turns into the Windows app's `Pitboard.ico`
  and the share package's logos.
- `website/public/`: `favicon.svg`, `favicon.ico`, `apple-touch-icon.png` and the `icon-*.png`
  files.
- `website/src/assets/logo/`: the lockup in the site's header and its preview images, and the mark
  in its drawn menu bar.

A page that moves needs a redirect in `docs.json`, so links from outside the site keep
working. Markdown files in the repository, such as `README.md`, `SECURITY.md`, `CHANGELOG.md`
and `packaging/tap-README.md`, and the pages in `website/` link pages by path. The README's
copies on crates.io and in release tarballs keep their links.

A redirect maps only a path, so a heading those files link, such as
`security#what-leaves-your-machine`, keeps its text. The macOS app's **Pitboard Help** item,
the Windows app's **Pitboard help** item and the Linux renewal unit link only the site's root,
so no move breaks them.
