"""Check every palette for legibility on the flat, dark UI.

    python3 -I scripts/theme-check.py [NAME ...] [--quiet] [--raw]

Reads assets/themes/*.nix and assets/wallpapers/*/theme.nix (the simple
attrsets those files are: `scheme = { ... };`, `extra = { ... };` and an
optional `overrides = { ... };`), applies `overrides` the way
assets/themes/default.nix does, plus `meta.themeOverrides` from
modules/flake/meta.nix for the theme that is active right now, and checks:

  text/bg       base05 on base00             >= 7    (WCAG AAA body text)
  dim/bg        base04 on base00             >= 4.5
  dim/surface   base04 on base01 (bar, panels)>= 4.5
  muted/bg      base03 on base00             >= 3    (labels, hints)
  muted/surface base03 on base01             >= 3
  accent/bg     base0D on base00             >= 4.5  (accent text, focus border)
                == base00 text on a base0D fill (contrast is symmetric)
  accent/surf   base0D on base01             >= 4.5
  red/bg        base08 on base00             >= 4
  border/bg     extra.border on base00       >= 1.4  (2px unfocused edges)
  surface step  base01 vs base00             >= 1.08
  raised step   base02 vs base01             >= 1.08
  accent chroma OKLCH chroma of base0D       >= 0.06 (else "accent has no colour")
  red chroma    OKLCH chroma of base08       >= 0.09 (urgent borders must read as red)
  colours       lowest chroma of base09-0C, base0E >= 0.07 (terminal colours, not greys)

WARN sits between the threshold and a softer floor; FAIL is below the floor.
--raw ignores `overrides` (shows the generated scheme on its own).
Exit status is 1 when anything FAILs. Standard library only.
"""

import math
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SLOTS = [f"base0{c}" for c in "0123456789ABCDEF"]
EXTRA = ["border", "accent2"]

# name, fg, bg, pass, warn-floor
CONTRAST = [
    ("text/bg", "base05", "base00", 7.0, 6.0),
    ("dim/bg", "base04", "base00", 4.5, 4.0),
    ("dim/surface", "base04", "base01", 4.5, 4.0),
    ("muted/bg", "base03", "base00", 3.0, 2.6),
    ("muted/surface", "base03", "base01", 3.0, 2.6),
    ("accent/bg", "base0D", "base00", 4.5, 4.0),
    ("accent/surf", "base0D", "base01", 4.5, 4.0),
    ("red/bg", "base08", "base00", 4.0, 3.5),
    ("border/bg", "border", "base00", 1.4, 1.25),
    ("surface step", "base01", "base00", 1.08, 1.04),
    ("raised step", "base02", "base01", 1.08, 1.04),
]
CHROMA_PASS, CHROMA_WARN = 0.06, 0.035
RED_PASS, RED_WARN = 0.09, 0.06
ANSI_PASS, ANSI_WARN = 0.07, 0.045
ANSI = ["base09", "base0A", "base0B", "base0C", "base0E"]


def block(text, name):
    """The `name = { ... };` attrset (flat, no nesting) as {key: value}."""
    m = re.search(r"(?m)^\s*" + name + r"\s*=\s*\{(.*?)\};", text, re.S)
    if not m:
        return None
    body = re.sub(r"#[^\n]*", "", m.group(1))
    return dict(re.findall(r'(\w+)\s*=\s*"([^"]*)"\s*;', body))


def load(path, global_over=None, raw=False):
    text = open(path, encoding="utf-8").read()
    scheme = block(text, "scheme") or {}
    extra = block(text, "extra") or {}
    layers = [] if raw else [block(text, "overrides") or {}]
    if global_over and not raw:
        layers.append(global_over)
    for over in layers:
        for k, v in over.items():
            (extra if k in EXTRA else scheme)[k] = v
    p = {k: scheme[k].upper() for k in SLOTS if k in scheme}
    p["border"] = (extra.get("border") or p.get("base02", "")).upper()
    return p, bool(layers and layers[0])


def lin(c):
    c /= 255
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4


def rgb(h):
    return [int(h[i : i + 2], 16) for i in (0, 2, 4)]


def lum(h):
    r, g, b = map(lin, rgb(h))
    return 0.2126 * r + 0.7152 * g + 0.0722 * b


def contrast(a, b):
    la, lb = sorted((lum(a), lum(b)), reverse=True)
    return (la + 0.05) / (lb + 0.05)


def oklch(h):
    r, g, b = map(lin, rgb(h))
    l = 0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b
    m = 0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b
    s = 0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b
    l, m, s = (x ** (1 / 3) for x in (l, m, s))
    L = 0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s
    A = 1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s
    B = 0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s
    return L, math.hypot(A, B), math.degrees(math.atan2(B, A)) % 360


def grade(v, ok, floor):
    return "PASS" if v >= ok else "WARN" if v >= floor else "FAIL"


def active_theme():
    """(theme file, meta.themeOverrides) of the configuration that is live."""
    try:
        text = open(os.path.join(ROOT, "modules/flake/meta.nix"), encoding="utf-8").read()
    except OSError:
        return None, None
    wall = re.search(r'(?m)^\s*wallpaper\s*=\s*"([^"]+)"\s*;', text)
    theme = re.search(r'(?m)^\s*theme\s*=\s*"([^"]+)"\s*;', text)
    over = block(text, "themeOverrides")
    if not theme:
        return None, over
    if theme.group(1) == "match":
        f = os.path.join(ROOT, "assets/wallpapers", wall.group(1) if wall else "", "theme.nix")
    else:
        f = os.path.join(ROOT, "assets/themes", theme.group(1) + ".nix")
    return os.path.realpath(f), over


def files():
    out = []
    d = os.path.join(ROOT, "assets/themes")
    for f in sorted(os.listdir(d)):
        if f.endswith(".nix") and f != "default.nix":
            out.append((f[:-4], os.path.join(d, f)))
    d = os.path.join(ROOT, "assets/wallpapers")
    for n in sorted(os.listdir(d)):
        f = os.path.join(d, n, "theme.nix")
        if os.path.isfile(f):
            out.append((n, f))
    return out


def main(argv):
    quiet = "--quiet" in argv
    raw = "--raw" in argv
    names = [a for a in argv if not a.startswith("--")]
    live, live_over = active_theme()
    tty = sys.stdout.isatty() and not os.environ.get("NO_COLOR")
    col = {"PASS": "32", "WARN": "33", "FAIL": "31"}

    def tag(g):
        return f"\033[{col[g]}m{g}\033[0m" if tty else g

    summary, failed = [], False
    for name, path in files():
        if names and name not in names:
            continue
        is_live = os.path.realpath(path) == live
        p, has_over = load(path, live_over if is_live else None, raw)
        missing = [k for k in SLOTS if k not in p]
        if missing:
            print(f"{name}: missing {', '.join(missing)}")
            failed = True
            continue
        rows = []
        for label, fg, bg, ok, floor in CONTRAST:
            v = contrast(p[fg], p[bg])
            rows.append((label, f"{v:5.2f}", f">= {ok}", grade(v, ok, floor), f"{p[fg]} on {p[bg]}"))
        _, c, hue = oklch(p["base0D"])
        g = grade(c, CHROMA_PASS, CHROMA_WARN)
        note = f"hue {hue:3.0f}" + ("  accent has no colour" if g != "PASS" else "")
        rows.append(("accent chroma", f"{c:5.3f}", f">= {CHROMA_PASS}", g, note))
        c = oklch(p["base08"])[1]
        rows.append(("red chroma", f"{c:5.3f}", f">= {RED_PASS}", grade(c, RED_PASS, RED_WARN), p["base08"]))
        c, k = min((oklch(p[k])[1], k) for k in ANSI)
        rows.append(("colours", f"{c:5.3f}", f">= {ANSI_PASS}", grade(c, ANSI_PASS, ANSI_WARN), f"lowest: {k} {p[k]}"))
        worst = "FAIL" if any(r[3] == "FAIL" for r in rows) else "WARN" if any(r[3] == "WARN" for r in rows) else "PASS"
        failed |= worst == "FAIL"
        flags = (" overrides" if has_over else "") + (" +meta.themeOverrides (live)" if is_live and live_over else "") + (" [live]" if is_live else "")
        bad = [r[0] for r in rows if r[3] != "PASS"]
        summary.append((name, worst, bad))
        if quiet and worst == "PASS":
            continue
        print(f"{tag(worst)}  {name}{flags}")
        for label, v, th, gr, note in rows:
            if quiet and gr == "PASS":
                continue
            print(f"    {tag(gr)}  {label:<14}{v}  {th:<8} {note}")
    print()
    for name, worst, bad in summary:
        print(f"  {tag(worst)}  {name:<14} {', '.join(bad)}")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
