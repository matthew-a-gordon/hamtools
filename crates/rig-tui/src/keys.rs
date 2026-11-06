//! The key table: the single source for shared-key dispatch, for the reserved plain-key check
//! and (later) for the keybindings modal. No rebinding.
//!
//! Three scopes. A [`Scope::Chord`] works from anywhere, from either focus. A [`Scope::Plain`]
//! key and a [`Scope::Alias`] (a plain key kept for an old Chord) act only when no text-entering
//! area has focus. F1 to F6 are [`Scope::Soft`]: the Activity's text area sees them from either
//! focus, so the table lists them but the host does not dispatch them.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Chord,
    Alias,
    Plain,
    Soft,
}

/// What a key does. Shared actions are performed by the host; [`Action::Activity`] names an
/// Activity's own action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    GoTo,
    Keybindings,
    Bandplan,
    RigSwitcher,
    LogForm,
    SessionList,
    Config,
    LocatorFromGps,
    Quit,
    FreqEntry,
    FreqDigit(char),
    SplitFreqEntry,
    TuneUp,
    TuneDown,
    StepUp,
    StepDown,
    ModeNext,
    ModePrev,
    ToggleVfo,
    ToggleSplit,
    SoftKey(u8),
    Activity(&'static str),
}

/// A key and its modifiers. Shift is not compared: a capital letter is its own character.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key {
    pub code: KeyCode,
    pub mods: KeyModifiers,
}

impl Key {
    pub const fn plain(code: KeyCode) -> Self {
        Self {
            code,
            mods: KeyModifiers::NONE,
        }
    }

    pub const fn ctrl(c: char) -> Self {
        Self {
            code: KeyCode::Char(c),
            mods: KeyModifiers::CONTROL,
        }
    }

    pub fn matches(&self, event: &KeyEvent) -> bool {
        let mods = event.modifiers - KeyModifiers::SHIFT;
        let code = match event.code {
            // Some terminals send Ctrl-letter as the capital.
            KeyCode::Char(c) if mods.contains(KeyModifiers::CONTROL) => {
                KeyCode::Char(c.to_ascii_lowercase())
            }
            other => other,
        };
        code == self.code && mods == self.mods
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyEntry {
    pub scope: Scope,
    pub key: Key,
    pub action: Action,
    pub label: &'static str,
    /// Why the key does nothing in some states, for the modal to show.
    pub blocked: Option<&'static str>,
}

const fn entry(scope: Scope, key: Key, action: Action, label: &'static str) -> KeyEntry {
    KeyEntry {
        scope,
        key,
        action,
        label,
        blocked: None,
    }
}

const fn chord(c: char, action: Action, label: &'static str) -> KeyEntry {
    entry(Scope::Chord, Key::ctrl(c), action, label)
}

const fn on(c: char, action: Action, label: &'static str) -> KeyEntry {
    entry(Scope::Plain, Key::plain(KeyCode::Char(c)), action, label)
}

const fn alias(c: char, action: Action, label: &'static str) -> KeyEntry {
    entry(Scope::Alias, Key::plain(KeyCode::Char(c)), action, label)
}

const fn digit(c: char) -> KeyEntry {
    on(c, Action::FreqDigit(c), "start frequency entry")
}

const fn soft(n: u8, label: &'static str) -> KeyEntry {
    entry(
        Scope::Soft,
        Key::plain(KeyCode::F(n)),
        Action::SoftKey(n),
        label,
    )
}

const SHARED: [KeyEntry; 41] = [
    chord('g', Action::GoTo, "go to"),
    chord('k', Action::Keybindings, "keybindings"),
    chord('b', Action::Bandplan, "bandplan"),
    chord('r', Action::RigSwitcher, "rig switcher"),
    chord('l', Action::LogForm, "log form"),
    chord('j', Action::SessionList, "session list"),
    chord('o', Action::Config, "config"),
    chord(
        'p',
        Action::LocatorFromGps,
        "Locator from GPS (this session)",
    ),
    chord('c', Action::Quit, "quit"),
    alias('l', Action::LogForm, "log form"),
    alias('L', Action::SessionList, "session list"),
    alias('c', Action::Config, "config"),
    KeyEntry {
        blocked: Some("q quits only in General; use Ctrl-C"),
        ..on('q', Action::Quit, "quit")
    },
    on('f', Action::FreqEntry, "frequency"),
    on('S', Action::SplitFreqEntry, "split transmit frequency"),
    entry(
        Scope::Plain,
        Key::plain(KeyCode::Right),
        Action::TuneUp,
        "tune up",
    ),
    entry(
        Scope::Plain,
        Key::plain(KeyCode::Left),
        Action::TuneDown,
        "tune down",
    ),
    entry(
        Scope::Plain,
        Key::plain(KeyCode::Up),
        Action::StepUp,
        "bigger step",
    ),
    entry(
        Scope::Plain,
        Key::plain(KeyCode::Down),
        Action::StepDown,
        "smaller step",
    ),
    on('m', Action::ModeNext, "next mode"),
    on('M', Action::ModePrev, "previous mode"),
    on('v', Action::ToggleVfo, "VFO"),
    on('s', Action::ToggleSplit, "split"),
    digit('0'),
    digit('1'),
    digit('2'),
    digit('3'),
    digit('4'),
    digit('5'),
    digit('6'),
    digit('7'),
    digit('8'),
    digit('9'),
    soft(1, "soft key 1"),
    soft(2, "soft key 2"),
    soft(3, "soft key 3"),
    soft(4, "soft key 4"),
    soft(5, "soft key 5"),
    soft(6, "soft key 6"),
    entry(
        Scope::Plain,
        Key::plain(KeyCode::Tab),
        Action::Activity("focus"),
        "move focus",
    ),
    entry(
        Scope::Plain,
        Key::plain(KeyCode::Esc),
        Action::Activity("focus"),
        "leave text area",
    ),
];

/// The shared table.
pub fn shared() -> &'static [KeyEntry] {
    &SHARED
}

/// The shared action a key event is bound to in `scopes`.
pub fn lookup(event: &KeyEvent, scopes: &[Scope]) -> Option<&'static KeyEntry> {
    SHARED
        .iter()
        .find(|e| scopes.contains(&e.scope) && e.key.matches(event))
}

/// The plain key the Satellite Activity owns (sat mode). No other Activity may use it.
pub const SATELLITE_KEY: char = 'Z';

/// Check an Activity's own key table against the reserved keys: nothing in the shared table (so
/// no Chord, Soft key or shared plain key), nothing twice, and `Z` only for Satellite.
pub fn validate_activity(name: &str, keys: &[KeyEntry]) -> Result<(), String> {
    for (i, e) in keys.iter().enumerate() {
        if let Some(shared) = SHARED.iter().find(|s| s.key == e.key) {
            return Err(format!(
                "{name}: {:?} is reserved by the shared keys ({})",
                e.key.code, shared.label
            ));
        }
        if e.key == Key::plain(KeyCode::Char(SATELLITE_KEY)) && name != "Satellite" {
            return Err(format!(
                "{name}: {SATELLITE_KEY} is reserved for Satellite's sat mode"
            ));
        }
        if keys[..i].iter().any(|k| k.key == e.key) {
            return Err(format!("{name}: {:?} is bound twice", e.key.code));
        }
    }
    Ok(())
}
