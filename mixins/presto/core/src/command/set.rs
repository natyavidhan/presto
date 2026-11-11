//! `HelpCommand` and `SetCommand`.

use super::args::Args;
use super::helpers::{Completions, clickable, paginate, span, suggesting};
use super::{Command, CommandError, Ctx};
use crate::player::{Span, chat_color};
use crate::settings::{SettingMeta, Settings};
use crate::utils::output::force_command_prefix;

pub struct Help;

impl Command for Help {
    fn names(&self) -> &[&'static str] {
        &["help", "?"]
    }

    fn execute(&self, c: &mut Ctx, label: &str, args: &mut Args) -> Result<(), CommandError> {
        args.require_max(1)?;
        if !args.has_any() || args.is::<i32>(0)? {
            let commands: Vec<&dyn Command> = super::all()
                .iter()
                .copied()
                .filter(|c| !c.hidden_from_help())
                .collect();
            let label = label.to_string();
            let prefix = force_command_prefix();
            paginate(
                c,
                args,
                &commands,
                |c| c.log("All Presto commands (clickable):"),
                |command| {
                    let names = command.names().join("/");
                    let name = command.names()[0];
                    let hover = format!("{names}\n{}\n\nClick to view full help", command.short_desc());
                    let run = format!("{prefix}{label} {name}");
                    vec![
                        clickable(name, chat_color::GRAY, run.clone(), hover.clone()),
                        clickable(format!(" - {}", command.short_desc()), chat_color::DARK_GRAY, run, hover),
                    ]
                },
                Some(format!("{prefix}{label}")),
            )
        } else {
            let name = args.get_string()?.to_lowercase();
            let command = super::get_command(&name).ok_or(CommandError::NotFound(name))?;
            c.log(&format!("{} - {}", command.names().join(" / "), command.short_desc()));
            c.log("");
            for line in command.long_desc() {
                c.log(&line);
            }
            c.log("");
            c.log_spans(vec![clickable(
                "Click to return to the help menu",
                chat_color::GRAY,
                format!("{}{label}", force_command_prefix()),
                "",
            )]);
            Ok(())
        }
    }

    fn tab_complete(&self, _c: &mut Ctx, _label: &str, args: &mut Args) -> Result<Vec<String>, CommandError> {
        if args.has_exactly(1) {
            return Ok(Completions::new().add_commands().filter_prefix(&args.get_string()?).build());
        }
        Ok(Vec::new())
    }

    fn short_desc(&self) -> String {
        "View all commands or help on specific ones".into()
    }

    fn long_desc(&self) -> Vec<String> {
        lines(&[
            "Using this command, you can view detailed help information on how to use certain commands of Presto.",
            "",
            "Usage:",
            "> help - Lists all commands and their short descriptions.",
            "> help <command> - Displays help information on a specific command.",
        ])
    }
}

pub fn lines(l: &[&str]) -> Vec<String> {
    l.iter().map(|s| s.to_string()).collect()
}

pub struct Set;

/// `settingTypeToString`.
fn type_name(s: &SettingMeta) -> &'static str {
    s.type_name
}

impl Command for Set {
    fn names(&self) -> &[&'static str] {
        &["set", "setting", "settings"]
    }

    fn execute(&self, c: &mut Ctx, _label: &str, args: &mut Args) -> Result<(), CommandError> {
        let arg = if args.has_any() {
            args.get_string()?.to_lowercase()
        } else {
            "list".to_string()
        };
        if arg == "s" || arg == "save" {
            c.b.bot.save_settings();
            c.log("Settings saved");
            return Ok(());
        }
        if arg == "load" || arg == "ld" {
            let file = if args.has_any() {
                args.get_string()?
            } else {
                "settings.txt".to_string()
            };
            let path = c.b.bot.directory.join(&file);
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            let errors = {
                let s = c.b.bot.settings_mut();
                for meta in s.modified() {
                    (meta.reset)(s);
                }
                s.apply_file(&text)
            };
            for e in errors {
                log::warn!("[Presto] {file}: {e}");
            }
            c.log(&format!("Settings reloaded from {file}"));
            return Ok(());
        }
        let view_modified = matches!(arg.as_str(), "m" | "mod" | "modified");
        let view_all = matches!(arg.as_str(), "all" | "l" | "list");
        if view_modified || view_all {
            let search = if args.has_any() && args.peek_as_or_none::<i32>(0)?.is_none() {
                args.get_string()?
            } else {
                String::new()
            };
            args.require_max(1)?;
            let settings = c.b.bot.settings.clone();
            let mut list: Vec<&'static SettingMeta> = if view_modified {
                settings.modified()
            } else {
                Settings::all().iter().collect()
            };
            let lower = search.to_lowercase();
            list.retain(|s| s.name.to_lowercase().contains(&lower));
            list.sort_by_key(|s| s.name.to_lowercase());
            let which = if view_modified { "modified " } else { "" };
            let header = if search.is_empty() {
                format!("All {which}settings:")
            } else {
                format!("All {which}settings containing the string '{search}':")
            };
            let prefix = settings.prefix.clone();
            paginate(
                c,
                args,
                &list,
                |c| c.log(&header),
                |s| {
                    let hover = format!(
                        "{}\nType: {}\n\nValue:\n{}\n\nDefault Value:\n{}",
                        s.name,
                        type_name(s),
                        (s.value)(&settings),
                        (s.default_value)()
                    );
                    let suggestion = format!("{prefix}set {} ", s.name);
                    vec![
                        suggesting(s.name, chat_color::GRAY, suggestion.clone(), hover.clone()),
                        suggesting(format!(" ({})", type_name(s)), chat_color::DARK_GRAY, suggestion, hover),
                    ]
                },
                Some(format!("{}set {arg} {search}", force_command_prefix())),
            )?;
            return Ok(());
        }
        args.require_max(1)?;
        let resetting = arg == "reset";
        let toggling = arg == "toggle";
        let doing_something = resetting || toggling;
        if resetting {
            if !args.has_any() {
                c.log("Please specify 'all' as an argument to reset to confirm you'd really like to do this");
                c.log("ALL settings will be reset. Use the 'set modified' or 'modified' commands to see what will be reset.");
                c.log("Specify a setting name instead of 'all' to only reset one setting");
            } else if args.peek_string(0)?.eq_ignore_ascii_case("all") {
                {
                    let s = c.b.bot.settings_mut();
                    for meta in s.modified() {
                        (meta.reset)(s);
                    }
                }
                c.log("All settings have been reset to their default values");
                c.b.bot.save_settings();
                return Ok(());
            }
        }
        if toggling {
            args.require_min(1)?;
        }
        let setting_name = if doing_something { args.get_string()? } else { arg.clone() };
        let Some(setting) = Settings::all().iter().find(|s| s.name.eq_ignore_ascii_case(&setting_name)) else {
            return Err(CommandError::invalid_type(&args.consumed(), "a valid setting"));
        };
        if !doing_something && !args.has_any() {
            let value = (setting.value)(&c.b.bot.settings);
            c.log(&format!("Value of setting {}:", setting.name));
            c.log(&value);
        } else {
            let old = (setting.value)(&c.b.bot.settings);
            if resetting {
                (setting.reset)(c.b.bot.settings_mut());
            } else if toggling {
                let Some(v) = (setting.as_bool)(&c.b.bot.settings) else {
                    return Err(CommandError::invalid_type_got(
                        &args.consumed(),
                        "a toggleable setting",
                        "some other setting",
                    ));
                };
                (setting.set)(c.b.bot.settings_mut(), if v { "false" } else { "true" })
                    .map_err(CommandError::Unhandled)?;
                c.log(&format!("Toggled setting {} to {}", setting.name, !v));
            } else {
                let new_value = args.get_string()?;
                if let Err(e) = (setting.set)(c.b.bot.settings_mut(), &new_value) {
                    log::debug!("[Presto] set {} {new_value}: {e}", setting.name);
                    return Err(CommandError::invalid_type(&args.consumed(), "a valid value"));
                }
            }
            if !toggling {
                let now = (setting.value)(&c.b.bot.settings);
                c.log(&format!(
                    "Successfully {} {} to {now}",
                    if resetting { "reset" } else { "set" },
                    setting.name
                ));
            }
            c.log_spans(vec![clickable(
                format!("Old value: {old}"),
                chat_color::GRAY,
                format!("{}set {} {old}", force_command_prefix(), setting.name),
                "Click to set the setting back to this value",
            )]);
            let s = c.b.bot.settings.clone();
            if matches!(setting.name, "chatControl" | "chatControlAnyway") && !s.chat_control && !s.chat_control_anyway {
                c.log_colored(
                    "Warning: Chat commands will no longer work. If you want to revert this change, use prefix control (if enabled) or click the old value listed above.",
                    chat_color::RED,
                );
            } else if setting.name == "prefixControl" && !s.prefix_control {
                c.log_colored(
                    "Warning: Prefixed commands will no longer work. If you want to revert this change, use chat control (if enabled) or click the old value listed above.",
                    chat_color::RED,
                );
            }
        }
        c.b.bot.save_settings();
        Ok(())
    }

    fn tab_complete(&self, c: &mut Ctx, _label: &str, args: &mut Args) -> Result<Vec<String>, CommandError> {
        if !args.has_any() {
            return Ok(Vec::new());
        }
        let arg = args.get_string()?;
        if args.has_exactly(1) && !matches!(args.peek_string(0)?.to_lowercase().as_str(), "s" | "save") {
            if arg.eq_ignore_ascii_case("reset") {
                return Ok(Completions::new()
                    .add_modified_settings(&c.b.bot.settings)
                    .prepend(["all"])
                    .filter_prefix(&args.get_string()?)
                    .build());
            }
            if arg.eq_ignore_ascii_case("toggle") {
                return Ok(Completions::new()
                    .add_toggleable_settings()
                    .filter_prefix(&args.get_string()?)
                    .build());
            }
            if matches!(arg.to_lowercase().as_str(), "ld" | "load") {
                let dir = c.b.bot.directory.clone();
                let typed = args.get_string()?;
                let files: Vec<String> = std::fs::read_dir(&dir)
                    .map(|d| {
                        d.filter_map(|e| e.ok())
                            .map(|e| e.file_name().to_string_lossy().to_string())
                            .collect()
                    })
                    .unwrap_or_default();
                return Ok(Completions::from(files)
                    .filter(|f| !f.contains(' '))
                    .filter_prefix(&typed)
                    .build());
            }
            if let Some(setting) = Settings::meta(&arg) {
                let settings = c.b.bot.settings.clone();
                if let Some(v) = (setting.as_bool)(&settings) {
                    let order = if v { ["true", "false"] } else { ["false", "true"] };
                    return Ok(Completions::from(order).filter_prefix(&args.get_string()?).build());
                }
                return Ok(vec![(setting.value)(&settings)]);
            }
        } else if !args.has_any() {
            return Ok(Completions::new()
                .add_settings()
                .sort_alphabetically()
                .prepend(["list", "modified", "reset", "toggle", "save", "load"])
                .filter_prefix(&arg)
                .build());
        }
        Ok(Vec::new())
    }

    fn short_desc(&self) -> String {
        "View or change settings".into()
    }

    fn long_desc(&self) -> Vec<String> {
        lines(&[
            "Using the set command, you can manage all of Presto's settings. Almost every aspect is controlled by these settings - go wild!",
            "",
            "Usage:",
            "> set - Same as `set list`",
            "> set list [page] - View all settings",
            "> set modified [page] - View modified settings",
            "> set <setting> - View the current value of a setting",
            "> set <setting> <value> - Set the value of a setting",
            "> set reset all - Reset ALL SETTINGS to their defaults",
            "> set reset <setting> - Reset a setting to its default",
            "> set toggle <setting> - Toggle a boolean setting",
            "> set save - Save all settings (this is automatic tho)",
            "> set load - Load settings from settings.txt",
            "> set load [filename] - Load settings from another file in your minecraft/presto",
        ])
    }
}

/// A line of one colour.
pub fn plain(text: impl Into<String>, color: u32) -> Vec<Span> {
    vec![span(text, color)]
}
