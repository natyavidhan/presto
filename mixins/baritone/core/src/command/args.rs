//! `ICommandArgument`, `CommandArguments` and `ArgConsumer`: a command line split into words, read one at a
//! time, each remembering where it was for error messages.

use super::CommandError;
use std::collections::VecDeque;

/// One word of a command line (`ICommandArgument`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Arg {
    /// Its position among the arguments, from 0 (-1 when unknown).
    pub index: i32,
    pub value: String,
    /// The line from this word to the end, spaces kept (`getRawRest`).
    pub raw_rest: String,
}

impl Arg {
    /// `CommandArguments.unknown`.
    pub fn unknown() -> Arg {
        Arg {
            index: -1,
            value: "<unknown>".into(),
            raw_rest: String::new(),
        }
    }

    /// `getAs`: the word read as a number or a boolean.
    pub fn parse<T: ArgType>(&self) -> Result<T, CommandError> {
        T::parse(&self.value).ok_or_else(|| CommandError::invalid_type(self, T::NAME))
    }

    /// `is`.
    pub fn is<T: ArgType>(&self) -> bool {
        T::parse(&self.value).is_some()
    }
}

/// A type a word can be read as (`IArgParser.Stateless`).
pub trait ArgType: Sized {
    /// The Java type's simple name, for messages.
    const NAME: &'static str;
    fn parse(s: &str) -> Option<Self>;
}

impl ArgType for i32 {
    const NAME: &'static str = "Integer";
    fn parse(s: &str) -> Option<Self> {
        s.parse().ok()
    }
}

impl ArgType for i64 {
    const NAME: &'static str = "Long";
    fn parse(s: &str) -> Option<Self> {
        s.parse().ok()
    }
}

/// Baritone's check before `parseDouble`: `[+-]?(\d+(\.\d*)?|\.\d+)` (no exponents, no `NaN`).
pub fn is_plain_decimal(s: &str) -> bool {
    let s = s.strip_prefix(['+', '-']).unwrap_or(s);
    let digits = |p: &str| p.bytes().all(|b| b.is_ascii_digit());
    match s.split_once('.') {
        None => !s.is_empty() && digits(s),
        // `\d+\.\d*` or `\.\d+`.
        Some((int, frac)) => digits(int) && digits(frac) && !(int.is_empty() && frac.is_empty()),
    }
}

impl ArgType for f64 {
    const NAME: &'static str = "Double";
    fn parse(s: &str) -> Option<Self> {
        if !is_plain_decimal(s) {
            return None;
        }
        s.parse().ok()
    }
}

impl ArgType for f32 {
    const NAME: &'static str = "Float";
    fn parse(s: &str) -> Option<Self> {
        if !is_plain_decimal(s) {
            return None;
        }
        s.parse().ok()
    }
}

/// `BooleanArgumentParser`.
pub const TRUTHY: [&str; 7] = ["1", "true", "yes", "t", "y", "on", "enable"];
pub const FALSY: [&str; 7] = ["0", "false", "no", "f", "n", "off", "disable"];

impl ArgType for bool {
    const NAME: &'static str = "Boolean";
    fn parse(s: &str) -> Option<Self> {
        let s = s.to_ascii_lowercase();
        if TRUTHY.contains(&s.as_str()) {
            Some(true)
        } else if FALSY.contains(&s.as_str()) {
            Some(false)
        } else {
            None
        }
    }
}

/// `CommandArguments.from`: the words of `s`; with `preserve_empty_last`, a trailing space adds an empty word
/// (what tab completion completes).
pub fn split(s: &str, preserve_empty_last: bool) -> Vec<Arg> {
    let mut args = Vec::new();
    let mut last_end = None;
    let mut rest = s;
    let mut offset = 0;
    loop {
        let trimmed = rest.trim_start();
        if trimmed.is_empty() {
            break;
        }
        let start = offset + (rest.len() - trimmed.len());
        let len = trimmed.find(char::is_whitespace).unwrap_or(trimmed.len());
        args.push(Arg {
            index: args.len() as i32,
            value: trimmed[..len].to_string(),
            raw_rest: s[start..].to_string(),
        });
        last_end = Some(start + len);
        offset = start + len;
        rest = &s[offset..];
    }
    if preserve_empty_last && last_end.is_none_or(|e| e < s.len()) {
        args.push(Arg {
            index: args.len() as i32,
            value: String::new(),
            raw_rest: String::new(),
        });
    }
    args
}

/// `CommandManager.expand`: the label (first word) and the arguments after it.
pub fn expand(s: &str, preserve_empty_last: bool) -> (String, Vec<Arg>) {
    let label = s.split(char::is_whitespace).next().unwrap_or("").to_string();
    let args = split(&s[label.len()..], preserve_empty_last);
    (label, args)
}

/// `ArgConsumer`: the arguments not read yet, and those read.
#[derive(Clone, Debug, Default)]
pub struct Args {
    args: VecDeque<Arg>,
    consumed: Vec<Arg>,
}

impl Args {
    pub fn new(args: Vec<Arg>) -> Self {
        Self {
            args: args.into(),
            consumed: Vec::new(),
        }
    }

    pub fn parse_line(s: &str) -> Self {
        Self::new(split(s, false))
    }

    /// The arguments not read yet.
    pub fn remaining(&self) -> impl Iterator<Item = &Arg> {
        self.args.iter()
    }

    pub fn len(&self) -> usize {
        self.args.len()
    }

    pub fn is_empty(&self) -> bool {
        self.args.is_empty()
    }

    pub fn has(&self, n: usize) -> bool {
        self.args.len() >= n
    }

    pub fn has_any(&self) -> bool {
        self.has(1)
    }

    pub fn has_at_most(&self, n: usize) -> bool {
        self.args.len() <= n
    }

    pub fn has_exactly(&self, n: usize) -> bool {
        self.args.len() == n
    }

    pub fn peek(&self, index: usize) -> Result<&Arg, CommandError> {
        self.require_min(index + 1)?;
        Ok(&self.args[index])
    }

    pub fn peek_string(&self, index: usize) -> Result<&str, CommandError> {
        Ok(&self.peek(index)?.value)
    }

    /// `peekAs`.
    pub fn peek_as<T: ArgType>(&self, index: usize) -> Result<T, CommandError> {
        self.peek(index)?.parse()
    }

    /// `peekAsOrNull`.
    pub fn peek_as_or_none<T: ArgType>(&self, index: usize) -> Result<Option<T>, CommandError> {
        Ok(self.peek(index)?.parse().ok())
    }

    /// `is`.
    pub fn is<T: ArgType>(&self, index: usize) -> Result<bool, CommandError> {
        Ok(self.peek(index)?.is::<T>())
    }

    pub fn get(&mut self) -> Result<Arg, CommandError> {
        self.require_min(1)?;
        let arg = self.args.pop_front().expect("checked");
        self.consumed.push(arg.clone());
        Ok(arg)
    }

    pub fn get_string(&mut self) -> Result<String, CommandError> {
        Ok(self.get()?.value)
    }

    /// `getAs`.
    pub fn get_as<T: ArgType>(&mut self) -> Result<T, CommandError> {
        self.get()?.parse()
    }

    /// `getAsOrDefault`: reads the next word as `T` if it is one, else leaves it and gives `default`.
    pub fn get_as_or<T: ArgType>(&mut self, default: T) -> Result<T, CommandError> {
        match self.peek(0)?.parse() {
            Ok(v) => {
                self.get()?;
                Ok(v)
            }
            Err(_) => Ok(default),
        }
    }

    /// `rawRest`: the line from the next word on.
    pub fn raw_rest(&self) -> &str {
        self.args.front().map_or("", |a| a.raw_rest.as_str())
    }

    pub fn require_min(&self, min: usize) -> Result<(), CommandError> {
        if self.args.len() < min {
            return Err(CommandError::NotEnoughArguments(min + self.consumed.len()));
        }
        Ok(())
    }

    pub fn require_max(&self, max: usize) -> Result<(), CommandError> {
        if self.args.len() > max {
            return Err(CommandError::TooManyArguments(max + self.consumed.len()));
        }
        Ok(())
    }

    pub fn require_exactly(&self, n: usize) -> Result<(), CommandError> {
        self.require_min(n)?;
        self.require_max(n)
    }

    pub fn has_consumed(&self) -> bool {
        !self.consumed.is_empty()
    }

    /// The last word read (`consumed`), or [`Arg::unknown`].
    pub fn consumed(&self) -> Arg {
        self.consumed.last().cloned().unwrap_or_else(Arg::unknown)
    }

    pub fn consumed_string(&self) -> String {
        self.consumed().value
    }

    /// `getDatatypeFor` / `getDatatypePost`: reads a value with `f`; any failure is reported against the
    /// next word (or the last one read) as "Expected `name`".
    pub fn datatype<T>(
        &mut self,
        name: &str,
        f: impl FnOnce(&mut Args) -> Result<T, String>,
    ) -> Result<T, CommandError> {
        match f(self) {
            Ok(v) => Ok(v),
            Err(cause) => {
                log::debug!("[Baritone] {name}: {cause}");
                let at = self.args.front().cloned().unwrap_or_else(|| self.consumed());
                Err(CommandError::invalid_type(&at, name))
            }
        }
    }

    /// `getDatatype...OrNull`: like [`Args::datatype`], but on failure nothing is read and it's `None`.
    pub fn datatype_or_none<T>(&mut self, f: impl FnOnce(&mut Args) -> Result<T, String>) -> Option<T> {
        let saved = self.clone();
        match f(self) {
            Ok(v) => Some(v),
            Err(_) => {
                *self = saved;
                None
            }
        }
    }

    /// `peekDatatype...OrNull`: what [`Args::datatype_or_none`] would give, reading nothing.
    pub fn peek_datatype_or_none<T>(&self, f: impl FnOnce(&mut Args) -> Result<T, String>) -> Option<T> {
        f(&mut self.clone()).ok()
    }
}

/// Reads a word for a datatype: [`Args::get_string`] with the error as a plain message.
pub fn word(args: &mut Args) -> Result<String, String> {
    args.get_string().map_err(|e| e.message())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_like_baritone() {
        let a = split("  goto  1 ~2  ", false);
        assert_eq!(a.len(), 3);
        assert_eq!(a[1].value, "1");
        assert_eq!(a[1].raw_rest, "1 ~2  ");
        assert_eq!(a[2].index, 2);
        let a = split("mine ", true);
        assert_eq!(a.len(), 2);
        assert_eq!(a[1].value, "");
        let (label, args) = expand("goto 1 2 3", false);
        assert_eq!(label, "goto");
        assert_eq!(args.len(), 3);
        assert_eq!(args[0].raw_rest, "1 2 3");
    }

    #[test]
    fn numbers_as_baritone_reads_them() {
        assert_eq!(f64::parse("1.5"), Some(1.5));
        assert_eq!(f64::parse(".5"), Some(0.5));
        assert_eq!(f64::parse("-2."), Some(-2.0));
        assert_eq!(f64::parse("1e5"), None);
        assert_eq!(f64::parse("NaN"), None);
        assert_eq!(f64::parse("."), None);
        assert_eq!(i32::parse("+7"), Some(7));
        assert_eq!(bool::parse("Yes"), Some(true));
        assert_eq!(bool::parse("disable"), Some(false));
    }

    #[test]
    fn consumer_errors_count_what_was_read() {
        let mut a = Args::parse_line("a b");
        a.get().unwrap();
        assert_eq!(a.require_max(0).unwrap_err(), CommandError::TooManyArguments(1));
        assert_eq!(a.require_min(3).unwrap_err(), CommandError::NotEnoughArguments(4));
        let e = a.get_as::<i32>().unwrap_err();
        assert_eq!(e.message(), "Error at argument #2: Expected Integer");
    }
}
