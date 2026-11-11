"""Generate Presto's settings table from Baritone's Settings.java.

Presto keeps Baritone's settings: their names, types, defaults and descriptions, with the bot called Presto in the
descriptions, a few defaults changed for speed (OVERRIDES), one setting renamed for the name (RENAMES) and a few
added (EXTRA).

Usage: python -I tools/gen_settings.py <path to Baritone>/src/api/java/baritone/api/Settings.java mixins/presto/core/src/settings/list.rs
"""
import re
import sys

src = open(sys.argv[1], encoding="utf-8").read()

# Java Color constants (java.awt.Color)
COLORS = {
    "WHITE": (255, 255, 255), "LIGHT_GRAY": (192, 192, 192), "GRAY": (128, 128, 128),
    "DARK_GRAY": (64, 64, 64), "BLACK": (0, 0, 0), "RED": (255, 0, 0), "PINK": (255, 175, 175),
    "ORANGE": (255, 200, 0), "YELLOW": (255, 255, 0), "GREEN": (0, 255, 0), "MAGENTA": (255, 0, 255),
    "CYAN": (0, 255, 255), "BLUE": (0, 0, 255),
}

SKIP = {"logger", "notifier", "toaster"}

# Settings named after the bot get Presto's name; `Settings::meta` still reads the old one from a settings file.
RENAMES = {"shortBaritonePrefix": "shortPrestoPrefix"}


def presto_doc(line: str) -> str:
    """A description line from Settings.java with the bot called Presto. Links to Baritone stay as they are."""
    parts = re.split(r'(<a href="[^"]*">|https?://\S+)', line)
    for i in range(0, len(parts), 2):
        part = parts[i]
        part = part.replace("[B]", "[P]")
        part = re.sub(r"\bBaritone\b", "Presto", part)
        part = re.sub(r"\bbaritone\b", "Presto", part)
        parts[i] = part
    return "".join(parts)


def snake(name: str) -> str:
    s = re.sub(r"([a-z0-9])([A-Z])", r"\1_\2", name)
    s = re.sub(r"([A-Z]+)([A-Z][a-z])", r"\1_\2", s)
    return s.lower()


def block_id(expr: str) -> str:
    m = re.fullmatch(r"Blocks\.([A-Z0-9_]+)(?:\.asItem\(\))?", expr.strip())
    if not m:
        m = re.fullmatch(r"Items\.([A-Z0-9_]+)", expr.strip())
    if not m:
        raise ValueError("unknown block expr: " + expr)
    return "minecraft:" + m.group(1).lower()


def clean_doc(doc: str):
    lines = []
    for line in doc.splitlines():
        line = line.strip()
        line = re.sub(r"^/\*\*", "", line)
        line = re.sub(r"\*/$", "", line)
        line = re.sub(r"^\*\s?", "", line)
        line = line.replace("<p>", "").replace("</p>", "")
        line = re.sub(r"\{@link ([^}]+)\}", r"`\1`", line)
        line = re.sub(r"\{@code ([^}]+)\}", r"`\1`", line)
        line = re.sub(r"</?(ul|li|b|i|em|code|br)\s*/?>", "", line)
        line = line.rstrip()
        lines.append(line)
    while lines and not lines[0]:
        lines.pop(0)
    while lines and not lines[-1]:
        lines.pop()
    # collapse multiple blank lines
    out = []
    for l in lines:
        if not l and out and not out[-1]:
            continue
        out.append(l)
    return out


# Find each setting with the javadoc that precedes it.
pattern = re.compile(
    r"(?P<doc>/\*\*(?:(?!\*/).)*?\*/\s*)?(?:@[A-Za-z]+\s*)*public final Setting<(?P<type>.+?)> (?P<name>[A-Za-z0-9_]+) = new Setting<>\((?P<default>.*?)\);\n",
    re.S,
)

entries = []
for m in pattern.finditer(src):
    name = m.group("name")
    if name in SKIP:
        continue
    jtype = m.group("type").strip()
    default = re.sub(r"//[^\n]*", "", m.group("default"))
    default = " ".join(default.split())
    doc = [presto_doc(line) for line in clean_doc(m.group("doc") or "")]
    name = RENAMES.get(name, name)
    entries.append((name, jtype, default, doc))

out = []
# Defaults Presto changes, for speed: Java Baritone idles between blocks and clicks; here the waits are off
# by default and each stays a setting, so a server that watches for them can have them back.
OVERRIDES = {
    "blockBreakSpeed": ("1", [
        "",
        "Java Baritone defaults to 6, idling a quarter of a second after every block it breaks. Presto",
        "defaults to 1: no wait. Raise it (and `randomActionDelayTicks`) for a vanilla-like rhythm.",
    ]),
    "rightClickSpeed": ("1", [
        "",
        "Java Baritone defaults to 4. Presto defaults to 1: a block placed every tick when bridging or",
        "pillaring. Raise it for vanilla's rhythm.",
    ]),
    "mineDropLoiterDurationMSThanksLouca": ("0", [
        "",
        "Java Baritone waits 250 ms after each ore. Presto doesn't by default; drops are still collected",
        "on the way with `mineScanDroppedItems`.",
    ]),
    "randomLooking113": ("0.0", [
        "",
        "Java Baritone defaults to 2 degrees, which makes aims at the edge of a block miss and try again.",
        "Presto aims exactly by default; raise it to look less robotic.",
    ]),
}

# Settings Presto adds, after Baritone's own.
EXTRA = [
    ("instantAim", "Boolean", "true", [
        "Act in the same tick as turning: when Presto turns to a block to break it or place against it,",
        "it does so that tick, as the game applies the turn before the click. Off, it turns one tick and",
        "acts the next, as Java Baritone does (a 50 ms pause before every block).",
    ]),
    ("randomActionDelayTicks", "Integer", "0", [
        "Up to this many extra ticks, at random, before each block broken or placed, on top of",
        "`blockBreakSpeed` and `rightClickSpeed`, for a less mechanical rhythm. 0 for none.",
    ]),
    ("syncPathingBudgetMS", "Long", "3", [
        "When a new path is needed, search for it right away on the game thread for up to this many",
        "milliseconds: short paths (to the next ore, around a corner) start the same tick instead of after a",
        "handoff to the search thread. A longer search carries on on a thread. 0 always searches on a thread.",
    ]),
]

names = {e[0] for e in entries}
for name in OVERRIDES:
    if name not in names:
        raise SystemExit("override for unknown setting " + name)
patched = []
for name, jtype, default, doc in entries:
    if name in OVERRIDES:
        new_default, note = OVERRIDES[name]
        default = new_default
        doc = doc + note
    patched.append((name, jtype, default, doc))
entries = patched + [(n, t, d, doc) for n, t, d, doc in EXTRA]

out.append("// Generated from Baritone's Settings.java by tools/gen_settings.py. Do not edit by hand: regenerate it")
out.append("// instead, so the names, defaults and descriptions stay Baritone's (with the bot called Presto).")
out.append("define_settings! {")
for name, jtype, default, doc in entries:
    if jtype == "Boolean":
        rty, val = "bool", default
    elif jtype == "Integer":
        rty, val = "i32", default
    elif jtype == "Long":
        rty = "i64"
        if default.startswith("TimeUnit.MINUTES.toSeconds("):
            val = str(int(re.search(r"\((\d+)\)", default).group(1)) * 60)
        else:
            val = default.rstrip("Ll")
    elif jtype == "Float":
        rty, val = "f32", default.rstrip("fF")
        if "." not in val and "e" not in val.lower():
            val += ".0"
    elif jtype == "Double":
        rty, val = "f64", default.rstrip("dD")
        if "." not in val and "e" not in val.lower():
            val += ".0"
    elif jtype == "String":
        rty, val = "String", "String::from(" + default + ")"
    elif jtype == "Color":
        c = COLORS[default.split(".")[1]]
        rty, val = "Color", f"Color::rgb({c[0]}, {c[1]}, {c[2]})"
    elif jtype in ("List<Block>", "List<Item>"):
        ids = re.findall(r"(?:Blocks|Items)\.[A-Z0-9_]+(?:\.asItem\(\))?", default)
        rty = "BlockList" if jtype == "List<Block>" else "ItemList"
        val = rty + "::of(&[" + ", ".join('"' + block_id(i) + '"' for i in ids) + "])"
    elif jtype == "List<String>":
        rty, val = "StringList", "StringList::default()"
    elif jtype == "Map<Block, List<Block>>":
        rty, val = "BlockMap", "BlockMap::default()"
    elif jtype == "Vec3i":
        nums = re.findall(r"-?\d+", default.replace("new Vec3i", ""))
        rty, val = "[i32; 3]", "[" + ", ".join(nums) + "]"
    elif jtype == "Rotation":
        rty, val = "SchematicRotation", "SchematicRotation::" + {"NONE": "None"}[default.split(".")[1]]
    elif jtype == "Mirror":
        rty, val = "SchematicMirror", "SchematicMirror::" + {"NONE": "None"}[default.split(".")[1]]
    else:
        raise SystemExit("unhandled type " + jtype + " for " + name)
    if rty in ("f32", "f64"):
        val = re.sub(r"^(-?)\.", r"\g<1>0.", val)
    for line in doc:
        out.append("    ///" + (" " + line if line else ""))
    out.append(f"    {snake(name)}: {rty} = {val} => \"{name}\";")
out.append("}")
open(sys.argv[2], "w", encoding="utf-8", newline="\n").write("\n".join(out) + "\n")
print(len(entries), "settings")
