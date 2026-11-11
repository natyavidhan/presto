//! Baritone's commands (`#goto`, `#mine`, `#set`...): `CommandManager`, the argument parsing and every command.

use crate::baritone::Baritone;
use crate::ctx::PlayerContext;

/// `CommandManager.execute`: runs one command line (without the prefix). False if there is no such command.
pub fn execute(b: &mut Baritone, _pc: Option<&PlayerContext>, line: &str) -> bool {
    b.bot.log_direct(&format!("Unknown command: {line}"));
    false
}
