#!/usr/bin/env python3
"""Cut a CityGML CityModel down to a few of its members.

Keeps the XML prolog, the root start tag, gml:boundedBy (the Envelope), the
top-level cityObjectMember elements named by 0-based document position, and
only those app:appearanceMember elements whose app:target references all
resolve to a gml:id inside the kept members.  Every kept element is copied
verbatim, so it is byte-identical to the source.

Usage: cut_down.py <source.gml> <output.gml> <members>
where <members> is a comma-separated list of indices and A-B ranges,
e.g. "0,19,20,22" or "1-25".

See README.md for the exact invocations that produced this directory.
"""
import re
import sys


def spans(src, name):
    """Byte spans of every top-level <name> ... </name> element."""
    out = []
    for m in re.finditer(r"<" + re.escape(name) + r"[\s>]", src):
        start = m.start()
        end = src.index("</" + name + ">", start) + len(name) + 3
        out.append((start, end))
    return out


def main():
    src_path, out_path, keep_spec = sys.argv[1], sys.argv[2], sys.argv[3]
    keep = set()
    for part in keep_spec.split(","):
        if "-" in part:
            a, b = part.split("-")
            keep.update(range(int(a), int(b) + 1))
        else:
            keep.add(int(part))

    src = open(src_path, encoding="utf-8").read()
    members = spans(src, "cityObjectMember")
    apps = spans(src, "app:appearanceMember")
    bounded = spans(src, "gml:boundedBy")

    # Everything before the first top-level child: prolog + root start tag.
    first = min(s for s, _ in members + apps + bounded)
    header = src[:first]
    root_local = re.findall(r"<([A-Za-z0-9_]*:?CityModel)[\s>]", header)[-1]

    kept_members = [src[a:b] for i, (a, b) in enumerate(members) if i in keep]
    ids = set()
    for chunk in kept_members:
        ids.update(re.findall(r'gml:id="([^"]+)"', chunk))

    kept_apps = []
    for a, b in apps:
        chunk = src[a:b]
        targets = re.findall(r"<app:target[^>]*>#?([^<\s\"]+)", chunk)
        targets += re.findall(r'<app:target[^>]*uri="#([^"]+)"', chunk)
        if targets and all(t.lstrip("#") in ids for t in targets):
            kept_apps.append(chunk)

    parts = [header]
    if bounded:
        parts.append(src[bounded[0][0]:bounded[0][1]] + "\n")
    parts.extend(c + "\n" for c in kept_apps)
    parts.extend(c + "\n" for c in kept_members)
    parts.append("</%s>\n" % root_local)
    open(out_path, "w", encoding="utf-8").write("".join(parts))
    print(
        "members kept: %d/%d, appearanceMembers kept: %d/%d, bytes: %d"
        % (len(kept_members), len(members), len(kept_apps), len(apps),
           sum(len(p) for p in parts))
    )


if __name__ == "__main__":
    main()
