# Prints the app's metainfo: the template given, with its @RELEASES@ line made AppStream 1.0's
# <releases>, one <release> for each released section of the changelog given from 1.0 on, in its
# order. The 0.x releases never had the Linux app, and a software centre shows the newest stable
# release listed as the app's (docs/facts/linux-app.md, The metainfo). Each kind of change is a
# <p> of its name over a <ul>, a bullet an <li>, a code span <code>, bold <em> and a link its
# text. Anything else a section holds is refused, never dropped, and so is a web address, which
# AppStream takes for a link a description cannot hold.
import re
import sys
from pathlib import Path
from xml.sax.saxutils import escape, quoteattr

VERSION = re.compile(r"## \[(\d+\.\d+\.\d+(?:-beta\.\d+)?)\] - (\d{4}-\d{2}-\d{2})\s*")
UNRELEASED = re.compile(r"## \[Unreleased\]\s*")
KIND = re.compile(r"### (Added|Changed|Deprecated|Removed|Fixed|Security)\s*")
LINK_DEFINITION = re.compile(r"\[[^\]]+\]:\s*\S+\s*")
WEB_ADDRESS = re.compile(r"(https?|ftp)://")
CODE = re.compile(r"`([^`]+)`")
LINK = re.compile(r"\[([^\]]+)\]\([^)\s]+\)")
BOLD = re.compile(r"\*\*(.+?)\*\*")
PLACE = "@RELEASES@"


class Refused(Exception):
    pass


def inline(text):
    parts = CODE.split(text)
    out = []
    for i, part in enumerate(parts):
        if i % 2:
            out.append(f"<code>{escape(part)}</code>")
            continue
        part = BOLD.sub(r"<em>\1</em>", LINK.sub(r"\1", escape(part)))
        if "`" in part or "**" in part or "](" in part:
            raise Refused(f"Markdown AppStream cannot hold: {text}")
        out.append(part)
    said = "".join(out)
    whole = re.fullmatch(r"_(.+)_", said)
    return f"<em>{whole.group(1)}</em>" if whole else said


# A section's lines as paragraphs and lists, each list's items its bullets' lines joined.
def blocks(lines, version):
    out, i = [], 0
    while i < len(lines):
        line = lines[i]
        if not line.strip():
            i += 1
        elif line.startswith("- "):
            items = []
            while i < len(lines) and lines[i].startswith("- "):
                item = [lines[i][2:].strip()]
                i += 1
                while i < len(lines) and lines[i].startswith("  ") and lines[i].strip():
                    if lines[i].lstrip().startswith("- "):
                        raise Refused(f"{version} has a list inside a list: {lines[i].strip()}")
                    item.append(lines[i].strip())
                    i += 1
                items.append(" ".join(item))
            out.append(("ul", items))
        elif re.match(r"\s*(```|#|>|\||\d+\.\s|[*+]\s)", line):
            raise Refused(f"{version} has what AppStream cannot hold: {line.strip()}")
        else:
            para = []
            while i < len(lines) and lines[i].strip() and not lines[i].startswith("- "):
                para.append(lines[i].strip())
                i += 1
            out.append(("p", " ".join(para)))
    return out


def releases(changelog):
    found, current, section = [], None, None
    for line in changelog.replace("\r\n", "\n").split("\n"):
        if UNRELEASED.fullmatch(line):
            current = None
            continue
        heading = VERSION.fullmatch(line)
        if heading and heading.group(1).startswith("0."):
            current = None
            continue
        if heading:
            current = {"version": heading.group(1), "date": heading.group(2), "intro": []}
            current["kinds"], section = [], current["intro"]
            found.append(current)
            continue
        if LINK_DEFINITION.fullmatch(line):
            current = None
            continue
        if line.startswith("## "):
            raise Refused(f"a version heading needs ## [x.y.z] - YYYY-MM-DD: {line}")
        if current is None:
            continue
        kind = KIND.fullmatch(line)
        if kind:
            section = []
            current["kinds"].append((kind.group(1), section))
            continue
        section.append(line)
    if not found:
        raise Refused("the changelog has no released section")
    return found


def described(release):
    version = release["version"]
    lines = []
    for block, body in blocks(release["intro"], version):
        lines += marked(version, block, body)
    for kind, section in release["kinds"]:
        said = blocks(section, version)
        if not said:
            raise Refused(f"{version} has an empty {kind}")
        lines.append(f"<p>{kind}</p>")
        for block, body in said:
            lines += marked(version, block, body)
    if not lines:
        raise Refused(f"{version} says nothing")
    return lines


def marked(version, block, body):
    for text in [body] if block == "p" else body:
        if WEB_ADDRESS.search(LINK.sub(r"\1", text)):
            raise Refused(f"{version} holds a web address, which AppStream refuses: {text}")
    if block == "p":
        return [f"<p>{inline(body)}</p>"]
    return ["<ul>", *(f"  <li>{inline(item)}</li>" for item in body), "</ul>"]


def element(found):
    lines = ["<releases>"]
    for release in found:
        kind = "development" if "-" in release["version"] else "stable"
        version, date = quoteattr(release["version"]), quoteattr(release["date"])
        lines.append(f"  <release version={version} date={date} type={quoteattr(kind)}>")
        lines += ["    <description>", *("      " + l for l in described(release))]
        lines += ["    </description>", "  </release>"]
    lines.append("</releases>")
    return lines


def metainfo(template, changelog):
    place = [i for i, line in enumerate(template.split("\n")) if line.strip() == PLACE]
    if len(place) != 1 or template.count(PLACE) != 1:
        raise Refused(f"the template needs {PLACE} once, on a line of its own")
    lines = template.split("\n")
    indent = lines[place[0]][: len(lines[place[0]]) - len(lines[place[0]].lstrip())]
    made = [indent + line for line in element(releases(changelog))]
    return "\n".join(lines[: place[0]] + made + lines[place[0] + 1 :])


def main():
    if len(sys.argv) != 3:
        print("usage: metainfo-releases.py <template> <changelog>", file=sys.stderr)
        return 2
    template, changelog = (Path(path).read_text(encoding="utf-8") for path in sys.argv[1:])
    try:
        sys.stdout.write(metainfo(template, changelog))
    except Refused as refused:
        print(f"metainfo-releases.py: {refused}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
