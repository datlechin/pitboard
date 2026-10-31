// Inlined and hashed into the CSP, so theme, platform, desktop and surface are set before the first
// paint.
(function () {
  var d = document.documentElement;
  function get(k) {
    try {
      return localStorage.getItem(k);
    } catch (e) {
      return null;
    }
  }
  var theme = get("theme");
  if (theme === "dark" || theme === "light") {
    d.setAttribute("data-theme", theme);
    var bg = theme === "dark" ? "#090b0b" : "#ffffff";
    [].forEach.call(document.querySelectorAll('meta[name="theme-color"]'), function (m) {
      m.setAttribute("content", bg);
    });
  }
  var ua = navigator.userAgent || "";
  var hint = (navigator.userAgentData && navigator.userAgentData.platform) || "";
  var mobile =
    /iPhone|iPad|iPod|Android/i.test(ua) ||
    /Android/i.test(hint) ||
    (/Macintosh/.test(ua) && navigator.maxTouchPoints > 1);
  if (mobile) d.setAttribute("data-os", "mobile");
  // a phone runs no Pitboard, so it sees the default, macOS
  var system = mobile ? "" : hint || ua;
  // ChromeOS's Linux may be older than the app's GTK needs, and the command line runs on it
  var chromeos = /CrOS|Chrome OS/i.test(system);
  // a link to a platform's part of a page shows that platform for this visit alone
  var linked = /^#(macos|windows|linux)$/.exec(location.hash);
  var platform = linked ? linked[1] : get("platform");
  if (!/^(macos|windows|linux)$/.test(platform)) {
    if (/Windows/i.test(system)) platform = "windows";
    else if (/Linux|X11/i.test(system) || chromeos) platform = "linux";
    else platform = "macos";
  }
  // no browser names the desktop, so GNOME unless the reader chose KDE Plasma
  var desktop = get("desktop") === "kde" ? "kde" : "gnome";
  d.setAttribute("data-desktop", desktop);
  // a link to a section that some systems do not show, as index.astro marks each, shows macOS for
  // this visit to a reader of another
  var only = { "#account-windows": "macos windows", "#desktop": "macos" };
  var reader = platform === "linux" ? desktop : platform;
  if (only.hasOwnProperty(location.hash) && only[location.hash].split(" ").indexOf(reader) < 0)
    platform = "macos";
  d.setAttribute("data-platform", platform);
  var tab = get("tab");
  if (!tab && chromeos && platform === "linux") tab = "cli";
  d.setAttribute("data-surface", tab === "cli" ? "cli" : "app");
  // the menu demo starts rewound, so its first frame never flashes the end state
  if (!matchMedia("(prefers-reduced-motion: reduce)").matches) d.setAttribute("data-demo", "start");
  d.classList.add("js");
})();
