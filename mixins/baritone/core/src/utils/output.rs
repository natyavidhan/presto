//! `Helper`: what Baritone prints (`logDirect`, `logDebug`), notifies (`logNotification`) and toasts
//! (`logToast`), collected each tick for the game to show.

use crate::player::{ChatLine, ClickEvent, Notification, Span, chat_color};
use crate::settings::Settings;

#[derive(Debug, Default)]
pub struct Output {
    pub chat: Vec<ChatLine>,
    pub notifications: Vec<Notification>,
    pub toasts: Vec<(String, String)>,
    /// Leave the server with this message (`disconnectOnArrival`).
    pub disconnect: Option<String>,
}

/// `Helper.FORCE_COMMAND_PREFIX`: what the commands Baritone's own clickable chat runs start with, so a click
/// runs them whatever `prefix` is. It is random per session, so a server's chat can't fake one.
pub fn force_command_prefix() -> &'static str {
    use std::hash::{BuildHasher, Hasher};
    static PREFIX: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    PREFIX.get_or_init(|| {
        // `RandomState` is seeded from the OS's randomness.
        let a = std::collections::hash_map::RandomState::new().build_hasher().finish();
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u64(a);
        format!("<<{a:016x}{:016x}>>", h.finish())
    })
}

/// `Helper.getPrefix`: `[Baritone]`, light purple in dark purple brackets.
pub fn prefix(settings: &Settings) -> Vec<Span> {
    vec![
        Span {
            text: "[".into(),
            color: Some(chat_color::DARK_PURPLE),
            ..Default::default()
        },
        Span {
            text: if settings.short_baritone_prefix { "B" } else { "Baritone" }.into(),
            color: Some(chat_color::LIGHT_PURPLE),
            ..Default::default()
        },
        Span {
            text: "]".into(),
            color: Some(chat_color::DARK_PURPLE),
            ..Default::default()
        },
    ]
}

impl Output {
    pub fn take(&mut self) -> Output {
        std::mem::take(self)
    }

    /// `logDirect(Component...)`: one line made of spans, with the prefix (or as a toast with `logAsToast`).
    pub fn log_spans(&mut self, settings: &Settings, spans: Vec<Span>) {
        if settings.log_as_toast {
            let text: String = spans.iter().map(|s| s.text.as_str()).collect();
            let title: String = prefix(settings).iter().map(|s| s.text.as_str()).collect();
            self.toasts.push((title, text));
            return;
        }
        let mut line = ChatLine::default();
        if !settings.use_message_tag {
            line.spans.extend(prefix(settings));
            line.spans.push(Span {
                text: " ".into(),
                ..Default::default()
            });
        }
        line.spans.extend(spans);
        self.chat.push(line);
    }

    /// `logDirect(String, ChatFormatting)`: each line of the message, in one colour.
    pub fn log_colored(&mut self, settings: &Settings, message: &str, color: u32) {
        for l in message.split('\n') {
            self.log_spans(
                settings,
                vec![Span {
                    text: l.replace('\t', "    "),
                    color: Some(color),
                    ..Default::default()
                }],
            );
        }
    }

    /// `logDirect(String)`: grey.
    pub fn log_direct(&mut self, settings: &Settings, message: &str) {
        self.log_colored(settings, message, chat_color::GRAY);
    }

    /// `logDebug`: only with `chatDebug`; always to the log file.
    pub fn log_debug(&mut self, settings: &Settings, message: &str) {
        log::debug!("[Baritone] {message}");
        if settings.chat_debug {
            let saved_toast = settings.log_as_toast;
            // Debug never goes to toasts (Helper.logDebug).
            if saved_toast {
                let mut line = ChatLine::default();
                line.spans.extend(prefix(settings));
                line.spans.push(Span {
                    text: format!(" {message}"),
                    color: Some(chat_color::GRAY),
                    ..Default::default()
                });
                self.chat.push(line);
            } else {
                self.log_direct(settings, message);
            }
        }
    }

    /// `logNotification`: only with `desktopNotifications`.
    pub fn log_notification(&mut self, settings: &Settings, message: &str, error: bool) {
        if settings.desktop_notifications {
            self.notifications.push(Notification {
                title: "Baritone".into(),
                message: message.into(),
                error,
            });
        }
    }

    /// `logToast`.
    pub fn log_toast(&mut self, title: &str, message: &str) {
        self.toasts.push((title.into(), message.into()));
    }

    /// A line that runs a command when clicked.
    pub fn clickable(text: impl Into<String>, color: u32, command: impl Into<String>, hover: impl Into<String>) -> Span {
        Span {
            text: text.into(),
            color: Some(color),
            hover: Some(hover.into()),
            click: Some(ClickEvent::RunCommand(command.into())),
            ..Default::default()
        }
    }
}
