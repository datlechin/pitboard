// What KDE Plasma 6.7.5 and Konsole 26.08.2 say around Pitboard on the page's Fedora 44, read
// from their sources at those tags (plasma-workspace, plasma-desktop, konsole) and from Fedora's
// kde-settings 44.0, whose locale gives the clock en_GB's date.
import { hm, shortDate } from "../lib/words.ts";

// Konsole's menu bar, which KXmlGui shows until it is turned off, as konsoleui.rc and
// sessionui.rc merge it: Plugins holds the plugins Konsole ships.
export const konsoleMenus = ["File", "Edit", "View", "Bookmarks", "Settings", "Plugins", "Help"];

// Konsole's toolbars in one row, each tool's Breeze icon beside its text: mainToolBar's New Tab and
// Split View, menus both, then after sessionToolbar's spacer Copy, Paste and Find; the hamburger
// menu hides while the menu bar shows.
export const konsoleTools: { icon: string; text: string; menu?: boolean }[][] = [
  [
    { icon: "actions/22/tab-new", text: "New Tab", menu: true },
    { icon: "actions/22/view-split-left-right", text: "Split View", menu: true },
  ],
  [
    { icon: "actions/22/edit-copy", text: "Copy" },
    { icon: "actions/22/edit-paste", text: "Paste" },
    { icon: "actions/22/edit-find", text: "Find" },
  ],
];

// Konsole's window title, its tab's "%d : %n" (Profile.cpp LocalTabTitleFormat): the folder, then
// the program in front. KMainWindow puts an em dash and "Konsole" after it, which the page leaves
// out, as it draws no em dash.
export const konsoleTitle = (program: string) => `~ : ${program}`;

// The panel's clock (digital-clock's main.xml): the time over the date in its short format, which
// en_GB writes 13/10/2026.
export const panelClock = (epoch: number) => ({ time: hm(epoch), date: shortDate(epoch) });

// A notification popup's close button, as HeadingButtons.qml names it.
export const CLOSE = "Close";
