# Presto

**Presto** is [Baritone](https://github.com/cabaletta/baritone), the Minecraft pathfinding bot, rebuilt in Rust as a
native mod for [Horizon](https://github.com/horizon-rust/horizon) — Minecraft: Java Edition 26.3 rewritten in Rust.
Java mods don't run on Horizon, so Presto is Baritone ported line by line to Rust and compiled into the game, made
to be faster: no idle ticks between blocks, path searches that never stall the game, chunk scans on every core.

It is a work in progress: walking, mining, following, exploring, waypoints and the commands that drive them are in;
the builder (`#build`, `#sel`), farming and elytra flight are not yet (see [Status](#status)).

## Install

Horizon builds native mods into its own copy of the game. Presto pins Horizon **Nightly 2026-11-08**
(`fdc87aecdbd116bf1e08adcd062fcbaea8c2e2ca`); the launcher builds on that commit.

From a clone of this repository:

```sh
horizon-launcher native-build path/to/this/repo/mixins/presto --trust-native-code
```

Once the repository is on GitHub, the Mods page's **Add from GitHub** (or `horizon-launcher mods add-github <url>
--trust-native-code`) installs it straight from the link. Native mods run with your user's rights: read the source
before you build it. The first native build downloads Rust and takes a while; later ones are quick.

## Use

Commands start with `#` in the chat (the `prefix` setting), as in Baritone, or `/presto <command>`:

| Command | |
| --- | --- |
| `#goto x y z`, `#goto x z`, `#goto y`, `#goto <block>` | Go to a place, or to the nearest block of a kind |
| `#mine [count] <block> [...]` | Mine blocks (`#mine 64 diamond_ore`) |
| `#tunnel` | Dig straight ahead |
| `#follow players`, `#follow player <name>`, `#follow entity <type>` | Follow |
| `#pickup [item ...]` | Pick up dropped items |
| `#explore [x z]`, `#explorefilter <file> [invert]` | Walk to chunks the cache hasn't seen |
| `#wp save/goto/list ...`, `#sethome`, `#home` | Waypoints |
| `#surface`, `#thisway <n>`, `#axis`, `#invert`, `#come` | Other goals |
| `#pause`, `#resume`, `#cancel` (`#stop`) | Control |
| `#set <setting> <value>`, `#<setting> <value>`, `#modified` | Settings, saved in `presto/settings.txt` in the game folder |
| `#help [command]` | Everything else |

Presto's chat lines start with `[Presto]` (`[P]` with `shortPrestoPrefix`) and are clickable where Baritone's are.
The settings are Baritone's, with the same names, so a Baritone `settings.txt` works too (its `shortBaritonePrefix` is
read as `shortPrestoPrefix`).

## Faster than Java Baritone, and still tunable

Mining with Java Baritone stops between blocks: it turns to a block, waits a tick to be looking at it, breaks it,
waits 5 more ticks (`blockBreakSpeed` 6), turns to the next one. Presto leaves those waits out by default:

- **`instantAim`** (new, `true`): the turn is applied before the game handles the tick's clicks, so Presto turns to a
  block and hits it, places against it or opens it in the same tick. Off, it waits to be looking at the block first,
  as Java Baritone does.
- **`blockBreakSpeed`** `1` (Java Baritone: `6`): ticks from one block breaking to hitting the next. The game's own
  5-tick wait after a broken block is skipped while Presto mines, as Java Baritone also does.
- **`rightClickSpeed`** `1` (Java Baritone: `4`): ticks between placements.
- **`mineDropLoiterDurationMSThanksLouca`** `0` (Java Baritone: `250`): time spent waiting for a mined ore's drop.
- **`randomLooking113`** `0` (Java Baritone: `2`): random jitter on the aim.
- **`randomActionDelayTicks`** (new, `0`): a random extra wait of up to this many ticks on each break and placement,
  for servers that watch for perfectly regular clicking.

Set the Java values back (`#blockBreakSpeed 6`, `#instantAim false`...) to look like vanilla Baritone.

Under the hood: path searches run on their own threads over shared, copy-on-write snapshots of the game's chunks,
reading whole 16×16×16 sections straight from their palettes; short searches finish inside the tick that asked
for them (`syncPathingBudgetMS`, 3 ms) so a path starts at once; block scans run on every core and skip sections
whose palette holds nothing wanted; the A* keeps its nodes in one flat arena.

## Status

| Part | State |
| --- | --- |
| Path finding (every movement: walk, ascend, descend, fall, pillar, parkour, diagonals, water bucket falls) | Ported |
| Pathing behaviour, path executor, splicing, planning ahead | Ported |
| Chunk cache (Baritone's region file format: caches are shared with Java Baritone's) and waypoints | Ported |
| Processes: custom goal, mine, get to block, follow, explore, backfill, pause | Ported |
| Commands: 37 of Baritone's 42 (all but `build`, `litematica`, `sel`, `farm`, `elytra`), with tab completion in the core | Ported (chat tab completion isn't wired to Horizon yet) |
| Path and goal rendering | Ported (the main parts) |
| Builder (`#build`, `#sel`, `#tunnel` with a size), farm, elytra | To do |
| Moving items to the hotbar (`allowInventory`) | To do in the glue |
| GPU acceleration (distance fields, bulk scans) | Planned |
| Settings GUI tab | Planned |

## Layout

| Path | What it is |
| --- | --- |
| `mixins/presto/core/` | `presto-core`: the bot itself, with no tie to the game. Built and tested on its own (`cargo test`), including a small player simulator that runs whole commands end to end. |
| `mixins/presto/glue.rs` | The glue, compiled into the game as `crate::game::presto`: reads the game for Presto each tick and applies what it asks for. |
| `mixins/presto/hooks/` | The eight small hooks horizon-mixins injects into the game (tick start and end, movement keys, mining delay, chat, commands, rendering). |
| `mixins/presto/horizon-mixin.toml` | The bundle's manifest. |
| `tools/gen_settings.py` | Generates the settings table from Baritone's `Settings.java`. |

Develop: `cargo test` builds and tests the core. To check the bundle against Horizon's source, stage it with
[horizon-mixins](https://github.com/horizon-rust/horizon-mixins) on an export of the pinned commit (Linux):

```sh
git -C horizon archive fdc87aecdbd116bf1e08adcd062fcbaea8c2e2ca | tar -x -C horizon-export
rm horizon-export/CLAUDE.md   # a symlink; horizon-mixins refuses links
echo fdc87aecdbd116bf1e08adcd062fcbaea8c2e2ca > horizon-export/.horizon-revision
horizon-mixins apply --workspace horizon-export --mods mixins --out stage
cargo check --locked -p horizon-client --manifest-path stage/Cargo.toml
```

## License

LGPL-3.0-or-later, as Baritone is. Presto is a port of Baritone, by leijurv, Brady and its contributors; it follows
Baritone's code closely and keeps its names in the docs, so the two read side by side.
