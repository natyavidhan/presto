# Baritone for Horizon

A Rust port of [Baritone](https://github.com/cabaletta/baritone), the Minecraft pathfinding bot, as a native mod
for [Horizon](https://github.com/horizon-rust/horizon) — Minecraft: Java Edition 26.3 rewritten in Rust.

Work in progress. The layout:

| Path | What it is |
| --- | --- |
| `mixins/baritone/core/` | `baritone-core`: Baritone itself (path finding, movements, processes, commands, settings, chunk cache), with no tie to the game. Built and tested on its own. |
| `mixins/baritone/` | The Horizon native mod bundle (horizon-mixins): the glue that reads the game for Baritone and applies what it asks for. |

Build and test the core:

```sh
cargo test
```

## License

LGPL-3.0-or-later, as Baritone is. Baritone is by leijurv, Brady and its contributors; this port follows its
code closely and keeps its names in the docs so the two read side by side.
