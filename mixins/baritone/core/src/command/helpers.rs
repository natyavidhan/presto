//! `TabCompleteHelper` and `Paginator`.

use super::args::Args;
use super::{CommandError, Ctx};
use crate::player::{ClickEvent, Span, chat_color};
use crate::settings::Settings;

/// `TabCompleteHelper`: a list of completions built up step by step.
#[derive(Clone, Debug, Default)]
pub struct Completions(Vec<String>);

impl Completions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from<S: Into<String>>(items: impl IntoIterator<Item = S>) -> Self {
        Self(items.into_iter().map(Into::into).collect())
    }

    pub fn append<S: Into<String>>(mut self, items: impl IntoIterator<Item = S>) -> Self {
        self.0.extend(items.into_iter().map(Into::into));
        self
    }

    pub fn prepend<S: Into<String>>(mut self, items: impl IntoIterator<Item = S>) -> Self {
        let mut v: Vec<String> = items.into_iter().map(Into::into).collect();
        v.append(&mut self.0);
        self.0 = v;
        self
    }

    pub fn map(mut self, f: impl Fn(String) -> String) -> Self {
        self.0 = self.0.into_iter().map(f).collect();
        self
    }

    pub fn filter(mut self, f: impl Fn(&str) -> bool) -> Self {
        self.0.retain(|s| f(s));
        self
    }

    /// `sortAlphabetically`: ignoring case.
    pub fn sort_alphabetically(mut self) -> Self {
        self.0.sort_by_key(|a| a.to_lowercase());
        self
    }

    /// `filterPrefix`: those starting with `prefix`, ignoring case.
    pub fn filter_prefix(self, prefix: &str) -> Self {
        let prefix = prefix.to_lowercase();
        self.filter(|s| s.to_lowercase().starts_with(&prefix))
    }

    /// `filterPrefixNamespaced`: for ids; `sto` matches `minecraft:stone`.
    pub fn filter_prefix_namespaced(self, prefix: &str) -> Self {
        let valid = prefix
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "_-./:".contains(c));
        if !valid || prefix.matches(':').count() > 1 {
            return Self::default();
        }
        let full = if prefix.contains(':') {
            prefix.to_string()
        } else {
            format!("minecraft:{prefix}")
        };
        self.filter_prefix(&full)
    }

    /// `addCommands`: every command's names.
    pub fn add_commands(self) -> Self {
        let mut names: Vec<&str> = Vec::new();
        for c in super::all() {
            for n in c.names() {
                if !names.contains(n) {
                    names.push(n);
                }
            }
        }
        self.append(names)
    }

    /// `addSettings`.
    pub fn add_settings(self) -> Self {
        let mut names: Vec<&str> = Settings::all().iter().map(|s| s.name).collect();
        names.sort_by_key(|a| a.to_lowercase());
        self.append(names)
    }

    /// `addModifiedSettings`.
    pub fn add_modified_settings(self, settings: &Settings) -> Self {
        let mut names: Vec<&str> = settings.modified().iter().map(|s| s.name).collect();
        names.sort_by_key(|a| a.to_lowercase());
        self.append(names)
    }

    /// `addToggleableSettings`: the Boolean ones.
    pub fn add_toggleable_settings(self) -> Self {
        let mut names: Vec<&str> = Settings::all().iter().filter(|s| s.is_bool()).map(|s| s.name).collect();
        names.sort_by_key(|a| a.to_lowercase());
        self.append(names)
    }

    pub fn build(self) -> Vec<String> {
        self.0
    }
}

/// A span of chat text.
pub fn span(text: impl Into<String>, color: u32) -> Span {
    Span {
        text: text.into(),
        color: Some(color),
        ..Default::default()
    }
}

/// A span that runs a command when clicked.
pub fn clickable(text: impl Into<String>, color: u32, command: impl Into<String>, hover: impl Into<String>) -> Span {
    crate::utils::output::Output::clickable(text, color, command, hover)
}

/// A span that puts a command in the chat box when clicked.
pub fn suggesting(text: impl Into<String>, color: u32, command: impl Into<String>, hover: impl Into<String>) -> Span {
    Span {
        text: text.into(),
        color: Some(color),
        hover: Some(hover.into()),
        click: Some(ClickEvent::SuggestCommand(command.into())),
        ..Default::default()
    }
}

/// `Paginator.paginate`: reads an optional page number, then shows that page of `entries` (`pageSize` 8),
/// with `<<` and `>>` that run `command_prefix <page>` when given.
pub fn paginate<T>(
    c: &mut Ctx,
    args: &mut Args,
    entries: &[T],
    pre: impl FnOnce(&mut Ctx),
    transform: impl Fn(&T) -> Vec<Span>,
    command_prefix: Option<String>,
) -> Result<(), CommandError> {
    const PAGE_SIZE: usize = 8;
    let max_page = (entries.len().max(1) - 1) / PAGE_SIZE + 1;
    let valid = |p: i32| p > 0 && p as usize <= max_page;
    args.require_max(1)?;
    let mut page = 1;
    if args.has_any() {
        page = args.get_as::<i32>()?;
        if !valid(page) {
            let consumed = args.consumed();
            return Err(CommandError::invalid_type_got(
                &consumed,
                &format!("a valid page (1-{max_page})"),
                &consumed.value,
            ));
        }
    }
    pre(c);
    let offset = (page as usize - 1) * PAGE_SIZE;
    for i in offset..offset + PAGE_SIZE {
        match entries.get(i) {
            Some(e) => c.log_spans(transform(e)),
            None => c.log_colored("--", chat_color::DARK_GRAY),
        }
    }
    let has_prev = command_prefix.is_some() && valid(page - 1);
    let has_next = command_prefix.is_some() && valid(page + 1);
    let pager = |label: &str, enabled: bool, target: i32, hover: &str| -> Span {
        match (&command_prefix, enabled) {
            (Some(prefix), true) => clickable(label, chat_color::GRAY, format!("{prefix} {target}"), hover),
            _ => span(label, chat_color::DARK_GRAY),
        }
    };
    c.log_spans(vec![
        pager("<<", has_prev, page - 1, "Click to view previous page"),
        span(" | ", chat_color::GRAY),
        pager(">>", has_next, page + 1, "Click to view next page"),
        span(format!(" {page}/{max_page}"), chat_color::GRAY),
    ]);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completions() {
        let c = Completions::from(["Stone", "sand", "dirt"])
            .filter_prefix("s")
            .sort_alphabetically()
            .build();
        assert_eq!(c, vec!["sand", "Stone"]);
        let c = Completions::from(["minecraft:stone", "minecraft:dirt"])
            .filter_prefix_namespaced("sto")
            .build();
        assert_eq!(c, vec!["minecraft:stone"]);
        assert!(Completions::new().add_commands().build().contains(&"goto".to_string()));
    }
}
