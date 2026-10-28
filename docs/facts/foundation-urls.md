# Foundation's URLs

Measured on macOS 27.0 on 5 October 2026, by giving the Swift `SiteLink` and `Handoff` of
0.7.0, and `URLComponents(string:)`, the same text as `pitboard-sites`. `pitboard-sites`
reads a link as Foundation does, so a link from outside means what it meant to the macOS
app.

- `URLComponents(string:)` splits a link by RFC 3986. A path, query or fragment holding a
  character RFC 3986 does not allow there is kept with every such character
  percent-encoded, a `%` among them, so an escape already in it is encoded too: `/%41 x` is
  kept as `/%2541%20x`. One holding none is kept as written, escapes and all. A second `#`
  is `%23` in the fragment.
- A host is never encoded: one with a character RFC 3986 does not allow, or a `%` that
  starts no escape, makes no link. A host in plain ASCII is percent-decoded and kept in its
  case, `claude.ai%00` included. One that is not, or that has an `xn--` label, goes through
  ICU's IDNA: `ｃｌａｕｄｅ.ai` is `claude.ai`, `xn--bcher-kva.de` reads back as `bücher.de`,
  and a joiner in a label makes no link. Anything between brackets is an IP literal.
- `user` and `password` are nil where their bytes are not UTF-8, `port` is nil where an
  `Int` cannot hold the number, and `path` is empty where its bytes are not UTF-8. So the
  Swift `SiteLink` opened `https://%FF@claude.ai/` and `https://claude.ai:99999999999999999999/`
  without what it dropped, took `claude.ai/magic-link/%FF` for no sign-in link, and opened
  `claude.ai/x/../%FF` without seeing its dot segment. The Swift `Handoff` read
  `pitboard://open/%FF?url=claude.ai` as `pitboard://open?url=claude.ai`. `pitboard-sites`
  refuses each.
- Swift compares strings by grapheme cluster, so a combining mark or a joiner right after a
  `/` makes one character with it. The Swift `SiteLink` took `claude.ai/magic-link/%CC%81`
  for no sign-in link, and `chat.com/` followed by a combining mark for no link at all. It
  counted a link's length in clusters too, and opened one of 4106 clusters and 8194
  scalars. `pitboard-sites` compares the text, and counts a link's length in Unicode
  scalars.
- `CharacterSet.whitespacesAndNewlines`, which trims a link, is Unicode's `White_Space` and
  U+200B ZERO WIDTH SPACE, checked over every scalar.
- The WHATWG URL standard, which the `url` crate and WebKit follow, reads a link otherwise.
  It resolves dot segments, takes `\` for `/` in an `https` link, drops a default port such
  as `:443`, finds a host in `https:claude.ai`, strips a tab or a newline and leaves a second
  `#` as it is. It reads a host whose last label is a number as an IPv4 address, and decodes
  a host's escapes before IDNA. So `pitboard-sites` asks `url` for IDNA alone, with a label
  after the host so that no host is read as an address.
- 450,000 generated links and Pitboard links, ASCII and not, had the same answer from both
  apart from: Pitboard links that `URL(string:)` refuses, which never reach the macOS app,
  though the Windows app, reading a Pitboard link from its command line as text, can be
  given one; the things above that `pitboard-sites` refuses; grapheme clusters; and hosts
  that are not ASCII, where ICU's and WHATWG's IDNA differ over empty labels, escapes and
  what a label may hold. Each such host was refused by both, though one named it otherwise,
  apart from `ｃ%EF%BD%8Caude.ai`: `pitboard-sites` decodes its escapes before IDNA, as the
  WHATWG standard does, and takes it for `claude.ai`, where the Swift `SiteLink` said it was
  on `cｌaude.ai`.
- `URL(string:)` reads a host otherwise than `URLComponents`, and an account window's rules
  compared the `URL` WebKit handed them, so `pitboard-sites`' `WebAddress` reads one as
  `URL.host` does, measured on 5 October 2026. A host in ASCII is percent-decoded and kept
  in its case, `xn--bcher-kva.de` and `XN--BCHER-KVA.de` as written, and so is one with an
  `xn--` label IDNA refuses, `xn--claude-.ai`, for which `URLComponents.host` is nil. A
  host that is not ASCII is given in IDNA's ASCII form, `аpple.com` with a Cyrillic а as
  `xn--pple-43d.com`, and one IDNA refuses makes no `URL`. An IP literal is given without
  its brackets. `https:///x`, `https:claude.ai/x`, `blob:` and `data:` links name no host.
  `port` is `0` for `:0`, and nil for `claude.ai:`; `user` is empty, not nil, for
  `https://@claude.ai/`. `WebAddress` reads two things otherwise: it keeps a port no `Int`
  holds, and names no host in `//claude.ai/x`, where `URL` reads `claude.ai` on no scheme.
- `URL.path` is the path percent-decoded, then without the slashes it ends in, but for the
  one a path of nothing else keeps: `/c/shared/` is `/c/shared`, `/a%20b/%2F` is `/a b`, `//`
  is `/`, and no path at all is empty. A path whose bytes are not UTF-8 is empty, and
  neither the query nor the fragment is part of it. Measured with Swift 6.4 on macOS 27.0
  on 6 October 2026, on eleven addresses on `pitboard-fixture://`, which
  `WebAddress::path` reads the same.
- `NSString` splits a file name into a base and an extension at its last `.`, and finds no
  extension where it would be empty or hold a space, or where the base would be empty, `.`
  or `..`: `....a` has the extension `a`, `...a` none. `lastPathComponent` drops the
  slashes at the end, and is `/` for slashes alone. Measured the same day on 98 names, with
  the Swift `DownloadCenter` naming 52 of them as a download, free and with its own name
  taken.
- A file `URL`'s `path` is decomposed: `URL(fileURLWithPath:)` and `appendingPathComponent`
  give `Báo cáo.pdf` as `Ba\u{301}o ca\u{301}o.pdf`. So the Swift `DownloadCenter`, which
  compared `URL`s, numbered a download whose name a running one had in either form, though
  not `\u{F900}.pdf` against `\u{8C48}.pdf`, its canonical decomposition, nor two names
  that differ only in case. APFS, on this Mac's volume that is not case-sensitive, takes
  each pair for one file: one made under either name is found under the other. Measured the
  same day. `download_destination` compares the names it reserved decomposed, so it numbers
  the U+F900 pair too, and takes two that differ only in case for two, as the Swift did.
- `CharacterSet.whitespaces`, which trims a `Content-Disposition`, is a tab, Unicode's
  `Zs` and U+200B ZERO WIDTH SPACE, checked over every scalar. It holds no newline.

## The account windows' records key

The macOS app keys each Pitboard directory's account-window records by
`URL(fileURLWithPath:).standardizedFileURL.path`, `WebEnvironment.recordKey`, and the model
takes that key as the app gives it, `AccountWindowsLaunch.key`. Measured on macOS 27.0.1
(26A434) with Swift 6.4 on 29 October 2026, and read in swift-foundation at 55b480fc
(`String+Path.swift`, `URL_Swift.swift`, `String+Internals.swift`), whose `FOUNDATION_FRAMEWORK`
parts the macOS build has and the repository leaves out:

- By name: doubled slashes are one, trailing ones go, `.` and `..` are resolved, `/..` is `/`,
  and a relative directory is joined to the current one. `~/x` is in the person's home; `~` and
  `~user/x` are relative names. The path is decomposed as its file system representation:
  `Báo cáo` is `Ba\u{301}o ca\u{301}o`.
- By the file system: where a component is `..`, links are resolved, through `getattrlist`'s
  `ATTR_CMN_FULLPATH`, but only where the whole path is there: `link/..` is the link's target's
  folder, `link/../nowhere` is resolved by name. A leading `/private/`, `/var/automount/` or
  `/private/var/automount/` goes where what follows is there and is not under `/Applications`,
  `/Library`, `/System`, `/Users`, `/Volumes`, `/bin`, `/cores`, `/dev`, `/opt`, `/private`,
  `/sbin` or `/usr`: `/private/tmp` is `/tmp`, `/private/var/nowhere` stays.
- The decomposition is HFS+'s, from a table in the framework alone. Of the 1,112,062
  scalars but NUL and `/`, each after `/a`, Foundation changed 12,197, and NFD by
  `unicode-normalization` 0.1.26 the same ones and U+105C9 and U+105E4, Todhri letters of
  Unicode 16. Both keep
  U+2000 to U+2FFF, U+F900 to U+FAFF and U+2F800 to U+2FAFF. Swift's combining classes, by
  which Foundation orders marks, are zero for 34 scalars Unicode 17 gives one, which
  `unicode-normalization` 0.1.26 has.
- A port of these rules to Rust, given 57 directories, said what Foundation said of all but
  `/System/Volumes/Data/Users/../Users`: Foundation's is `/Users`, `std::fs::canonicalize`'s
  `/System/Volumes/Data/Users`.

So the key rests on the file system and on Apple's and Swift's Unicode tables, and only
Foundation can make it: the model leaves it to the app.
`theRecordsKeyIsFoundationsByNameAndByTheFileSystem` holds the rules above.

