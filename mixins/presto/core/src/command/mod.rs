//! Presto's commands: `ICommand`, `CommandManager`, the chat control of `ExampleBaritoneControl`, and every
//! command (`#goto`, `#mine`, `#set`...).

pub mod args;
pub mod datatypes;
pub mod defaults;
pub mod helpers;
pub mod set;
pub mod waypoints;

use crate::presto::Presto;
use crate::ctx::PlayerContext;
use crate::player::{Span, chat_color};
use crate::process::ProcessCtx;
use crate::settings::Settings;
use crate::utils::output::force_command_prefix;
use args::{Arg, Args, expand, split};
use helpers::Completions;
use std::fmt;

/// `ICommandException`s.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandError {
    /// `CommandInvalidArgumentException` (`CommandInvalidTypeException`): a word that isn't what was expected.
    InvalidArgument { index: i32, message: String },
    /// `CommandInvalidStateException`.
    InvalidState(String),
    /// `CommandNotEnoughArgumentsException`: at least this many.
    NotEnoughArguments(usize),
    /// `CommandTooManyArgumentsException`: at most this many.
    TooManyArguments(usize),
    /// `CommandNotFoundException`.
    NotFound(String),
    /// A `CommandErrorMessageException` with its own message.
    Message(String),
    /// `CommandUnhandledException`: a bug.
    Unhandled(String),
}

impl CommandError {
    /// `new CommandInvalidTypeException(arg, expected)`.
    pub fn invalid_type(arg: &Arg, expected: &str) -> Self {
        CommandError::InvalidArgument {
            index: arg.index,
            message: format!("Expected {expected}"),
        }
    }

    /// `new CommandInvalidTypeException(arg, expected, got)`.
    pub fn invalid_type_got(arg: &Arg, expected: &str, got: &str) -> Self {
        CommandError::InvalidArgument {
            index: arg.index,
            message: format!("Expected {expected}, but got {got} instead"),
        }
    }

    pub fn message(&self) -> String {
        match self {
            CommandError::InvalidArgument { index, message } => format!(
                "Error at argument #{}: {message}",
                if *index == -1 {
                    "<unknown>".to_string()
                } else {
                    (index + 1).to_string()
                }
            ),
            CommandError::InvalidState(m) | CommandError::Message(m) | CommandError::Unhandled(m) => m.clone(),
            CommandError::NotEnoughArguments(n) => format!("Not enough arguments (expected at least {n})"),
            CommandError::TooManyArguments(n) => format!("Too many arguments (expected at most {n})"),
            CommandError::NotFound(c) => format!("Command not found: {c}"),
        }
    }

    /// `handle`: how the player hears about it.
    pub fn handle(&self, b: &mut Presto) {
        match self {
            CommandError::NotFound(_) => b.bot.log_direct(&self.message()),
            CommandError::Unhandled(m) => {
                log::error!("[Presto] unhandled command exception: {m}");
                b.bot.log_colored(
                    "An unhandled exception occurred. The error is in your game's log.",
                    chat_color::RED,
                );
            }
            _ => b.bot.log_colored(&self.message(), chat_color::RED),
        }
    }
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message())
    }
}

/// What a command runs with: all of Presto, and the game as it is this tick (`None` outside a world).
pub struct Ctx<'a, 'b> {
    pub b: &'a mut Presto,
    pub pc: Option<&'a PlayerContext<'b>>,
}

impl<'a, 'b> Ctx<'a, 'b> {
    /// The player and the world; commands that need them fail outside a world.
    pub fn pc(&self) -> Result<&'a PlayerContext<'b>, CommandError> {
        self.pc
            .ok_or_else(|| CommandError::InvalidState("You need to be in a world for that".into()))
    }

    pub fn log(&mut self, msg: &str) {
        self.b.bot.log_direct(msg);
    }

    pub fn log_colored(&mut self, msg: &str, color: u32) {
        self.b.bot.log_colored(msg, color);
    }

    pub fn log_spans(&mut self, spans: Vec<Span>) {
        let settings = self.b.bot.settings.clone();
        self.b.bot.out.log_spans(&settings, spans);
    }

    pub fn settings(&self) -> &Settings {
        &self.b.bot.settings
    }

    /// Runs `f` with the processes and a [`ProcessCtx`] (calls into a process, as Java's commands make).
    pub fn with_processes<R>(
        &mut self,
        f: impl FnOnce(&mut crate::process::Processes, &mut ProcessCtx) -> R,
    ) -> Result<R, CommandError> {
        let pc = self.pc()?;
        let b = &mut *self.b;
        let mut c = ProcessCtx { bot: &mut b.bot, pc };
        Ok(f(&mut b.procs, &mut c))
    }

    /// `CommandManager.execute(String)`: runs another command line; false if there is no such command.
    pub fn execute(&mut self, line: &str) -> bool {
        execute(self.b, self.pc, line)
    }
}

/// `ICommand`.
pub trait Command: Send + Sync {
    /// Its names, lowercase: the first is its main one.
    fn names(&self) -> &[&'static str];
    fn short_desc(&self) -> String;
    fn long_desc(&self) -> Vec<String>;
    fn execute(&self, c: &mut Ctx, label: &str, args: &mut Args) -> Result<(), CommandError>;
    fn tab_complete(&self, _c: &mut Ctx, _label: &str, _args: &mut Args) -> Result<Vec<String>, CommandError> {
        Ok(Vec::new())
    }
    fn hidden_from_help(&self) -> bool {
        false
    }
}

/// `CommandAlias`: a name for another command line.
pub struct Alias {
    pub names: &'static [&'static str],
    pub short: &'static str,
    pub target: &'static str,
}

impl Command for Alias {
    fn names(&self) -> &[&'static str] {
        self.names
    }
    fn short_desc(&self) -> String {
        self.short.into()
    }
    fn long_desc(&self) -> Vec<String> {
        vec![format!("This command is an alias, for: {} ...", self.target)]
    }
    fn execute(&self, c: &mut Ctx, _label: &str, args: &mut Args) -> Result<(), CommandError> {
        let line = format!("{} {}", self.target, args.raw_rest());
        c.execute(&line);
        Ok(())
    }
    fn tab_complete(&self, c: &mut Ctx, _label: &str, args: &mut Args) -> Result<Vec<String>, CommandError> {
        let line = format!("{} {}", self.target, args.raw_rest());
        Ok(tab_complete_command(c, &line))
    }
}

/// Every command, in `DefaultCommands` order (the order `#help` lists them in).
pub fn all() -> &'static [&'static dyn Command] {
    use defaults::*;
    static ALL: &[&dyn Command] = &[
        &set::Help,
        &set::Set,
        &Alias {
            names: &["modified", "mod", "presto", "modifiedsettings"],
            short: "List modified settings",
            target: "set modified",
        },
        &Alias {
            names: &["reset"],
            short: "Reset all settings or just one",
            target: "set reset",
        },
        &GoalCommand,
        &GotoCommand,
        &PathCommand,
        &ProcCommand,
        &EtaCommand,
        &VersionCommand,
        &RepackCommand,
        &ComeCommand,
        &AxisCommand,
        &ForceCancelCommand,
        &GcCommand,
        &InvertCommand,
        &TunnelCommand,
        &RenderCommand,
        &FollowCommand,
        &PickupCommand,
        &ExploreFilterCommand,
        &ReloadAllCommand,
        &SaveAllCommand,
        &ExploreCommand,
        &BlacklistCommand,
        &FindCommand,
        &MineCommand,
        &ClickCommand,
        &SurfaceCommand,
        &ThisWayCommand,
        &waypoints::Waypoints,
        &Alias {
            names: &["sethome"],
            short: "Sets your home waypoint",
            target: "waypoints save home",
        },
        &Alias {
            names: &["home"],
            short: "Path to your home waypoint",
            target: "waypoints goto home",
        },
        &PauseCommand,
        &ResumeCommand,
        &PausedCommand,
        &CancelCommand,
    ];
    ALL
}

/// `CommandManager.getCommand`: by any of its names, ignoring case.
pub fn get_command(name: &str) -> Option<&'static dyn Command> {
    let name = name.to_lowercase();
    all().iter().rev().copied().find(|c| c.names().contains(&name.as_str()))
}

/// `CommandManager.execute(String)`: runs a command line (without the prefix); false if there is no such
/// command. Errors are told to the player.
pub fn execute(b: &mut Presto, pc: Option<&PlayerContext>, line: &str) -> bool {
    let (label, args) = expand(line, false);
    execute_expanded(b, pc, &label, args)
}

fn execute_expanded(b: &mut Presto, pc: Option<&PlayerContext>, label: &str, args: Vec<Arg>) -> bool {
    let Some(command) = get_command(label) else {
        return false;
    };
    let mut args = Args::new(args);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut c = Ctx { b: &mut *b, pc };
        command.execute(&mut c, label, &mut args)
    }));
    let error = match result {
        Ok(Ok(())) => None,
        Ok(Err(e)) => Some(e),
        Err(panic) => Some(CommandError::Unhandled(
            panic
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_default(),
        )),
    };
    if let Some(e) = error {
        e.handle(b);
    }
    true
}

/// `ExampleBaritoneControl.logRanCommand`: `> goto 1 2 3`, click to run it again.
fn log_ran_command(b: &mut Presto, command: &str, rest: &str) {
    let s = b.bot.settings.clone();
    if !s.echo_commands {
        return;
    }
    let msg = format!("{command}{rest}");
    let shown = if s.censor_ran_commands {
        format!("{command} ...")
    } else {
        msg.clone()
    };
    let span = crate::utils::output::Output::clickable(
        format!("> {shown}"),
        chat_color::WHITE,
        format!("{}{msg}", force_command_prefix()),
        "Click to rerun command",
    );
    b.bot.out.log_spans(&s, vec![span]);
}

/// `ExampleBaritoneControl.runCommand`: a line typed in the chat. A setting's name alone shows (or for a
/// Boolean, toggles) it; with one word after, sets it. True when it was Presto's.
pub fn run_command(b: &mut Presto, pc: Option<&PlayerContext>, msg: &str) -> bool {
    if msg.trim().eq_ignore_ascii_case("damn") {
        b.bot.log_direct("daniel");
        return false;
    }
    if msg.trim().eq_ignore_ascii_case("orderpizza") {
        // Java Baritone opens Domino's in the browser here.
        return false;
    }
    if msg.is_empty() {
        return run_command(b, pc, "help");
    }
    let (command, args) = expand(msg, false);
    let rest = &msg[command.len()..];
    if args.is_empty() {
        if let Some(setting) = Settings::meta(&command) {
            log_ran_command(b, &command, rest);
            let line = if setting.is_bool() {
                format!("set toggle {}", setting.name)
            } else {
                format!("set {}", setting.name)
            };
            execute(b, pc, &line);
            return true;
        }
    } else if args.len() == 1
        && let Some(setting) = Settings::all().iter().find(|s| s.name.eq_ignore_ascii_case(&command))
    {
        log_ran_command(b, &command, rest);
        execute(b, pc, &format!("set {} {}", setting.name, args[0].value));
        return true;
    }
    if get_command(&command).is_some() {
        log_ran_command(b, &command, rest);
    }
    execute_expanded(b, pc, &command, args)
}

/// Whether [`run_command`] would take this line, worked out without running it, so the chat can decide at
/// once whether to send it (`chatControl`).
pub fn is_command(msg: &str) -> bool {
    if msg.is_empty() {
        return true;
    }
    let (command, args) = expand(msg, false);
    if args.len() <= 1 && Settings::meta(&command).is_some() {
        return true;
    }
    get_command(&command).is_some()
}

/// `CommandManager.tabComplete(String)`.
pub fn tab_complete_command(c: &mut Ctx, line: &str) -> Vec<String> {
    let (label, args) = expand(line, true);
    if args.is_empty() {
        return Completions::new()
            .add_commands()
            .filter_prefix(&label)
            .build();
    }
    let Some(command) = get_command(&label) else {
        return Vec::new();
    };
    let mut args = Args::new(args);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| command.tab_complete(c, &label, &mut args)));
    match result {
        Ok(Ok(v)) => v,
        Ok(Err(_)) => Vec::new(),
        Err(_) => {
            log::error!("[Presto] tab completion of {line:?} panicked");
            Vec::new()
        }
    }
}

/// `ExampleBaritoneControl.tabComplete`: completions for a chat line without the prefix.
pub fn tab_complete(c: &mut Ctx, msg: &str) -> Vec<String> {
    let mut argc = Args::new(split(msg, true));
    if argc.has_at_most(2) {
        if argc.has_exactly(1) {
            let word = argc.get_string().unwrap_or_default();
            return Completions::new()
                .add_commands()
                .add_settings()
                .filter_prefix(&word)
                .build();
        }
        let first = argc.get_string().unwrap_or_default();
        if let Some(setting) = Settings::meta(&first) {
            let word = argc.get_string().unwrap_or_default();
            let settings = c.b.bot.settings.clone();
            if let Some(value) = (setting.as_bool)(&settings) {
                let order = if value { ["true", "false"] } else { ["false", "true"] };
                return Completions::from(order).filter_prefix(&word).build();
            }
            return vec![(setting.value)(&settings)];
        }
    }
    tab_complete_command(c, msg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique() {
        let mut names: Vec<&str> = all().iter().flat_map(|c| c.names().iter().copied()).collect();
        let len = names.len();
        names.sort();
        names.dedup();
        assert_eq!(len, names.len());
        assert!(get_command("GOTO").is_some());
        assert!(get_command("wp").is_some());
        assert!(get_command("nope").is_none());
    }

    #[test]
    fn recognises_command_lines() {
        assert!(is_command("goto 1 2 3"));
        assert!(is_command("allowBreak false"));
        assert!(is_command("allowbreak"));
        assert!(!is_command("allowBreak false true"));
        assert!(!is_command("hello there"));
    }
}
