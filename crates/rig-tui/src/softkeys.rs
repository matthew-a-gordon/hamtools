//! Soft key templates: the variables, the one shared validator, expansion at press time, and the
//! per-key layering of where a message comes from.
//!
//! Everything here is pure data in and data out. Nothing in this module can reach the rig:
//! [`expand`] returns text or a refusal, and the only route to the transmitter stays a key-press
//! `PanelCommand::Send`. The config dialog and the future plan editor call [`validate`] directly.
//!
//! `{MYRST}` is the report the operator gives (RST sent) and `{THEIRRST}` the one received.

use std::collections::BTreeMap;
use std::fmt;

/// Messages longer than this after worst-case expansion get a warning (never a refusal).
pub const LONG_MESSAGE: usize = 120;

/// The number of Soft key slots, F1 to F6.
pub const SLOTS: u8 = 6;

/// A variable a message may contain, written `{NAME}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variable {
    MyCall,
    TheirCall,
    MyRef,
    TheirRef,
    MyRst,
    TheirRst,
}

impl Variable {
    pub const ALL: [Variable; 6] = [
        Variable::MyCall,
        Variable::TheirCall,
        Variable::MyRef,
        Variable::TheirRef,
        Variable::MyRst,
        Variable::TheirRst,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Variable::MyCall => "MYCALL",
            Variable::TheirCall => "THEIRCALL",
            Variable::MyRef => "MYREF",
            Variable::TheirRef => "THEIRREF",
            Variable::MyRst => "MYRST",
            Variable::TheirRst => "THEIRRST",
        }
    }

    fn parse(name: &str) -> Option<Variable> {
        Variable::ALL
            .into_iter()
            .find(|v| v.name().eq_ignore_ascii_case(name))
    }

    /// The longest a value can plausibly be, for the length warning.
    fn worst_case_len(self) -> usize {
        match self {
            Variable::MyCall | Variable::TheirCall => 13,
            Variable::MyRef | Variable::TheirRef => 12,
            Variable::MyRst | Variable::TheirRst => 3,
        }
    }
}

/// The live values variables expand from. `None` or blank means "not known right now".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Live<'a> {
    pub my_call: Option<&'a str>,
    pub their_call: Option<&'a str>,
    pub my_ref: Option<&'a str>,
    pub their_ref: Option<&'a str>,
    pub my_rst: Option<&'a str>,
    pub their_rst: Option<&'a str>,
}

impl<'a> Live<'a> {
    fn get(&self, variable: Variable) -> Option<&'a str> {
        let value = match variable {
            Variable::MyCall => self.my_call,
            Variable::TheirCall => self.their_call,
            Variable::MyRef => self.my_ref,
            Variable::TheirRef => self.their_ref,
            Variable::MyRst => self.my_rst,
            Variable::TheirRst => self.their_rst,
        };
        value.map(str::trim).filter(|v| !v.is_empty())
    }
}

/// Something wrong with a message that makes it unsendable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    UnknownVariable(String),
    /// `{CALL}` was removed in favour of `{MYCALL}`.
    CallRemoved,
    UnbalancedBrace,
    UnsendableChar(char),
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Problem::UnknownVariable(name) => write!(f, "unknown variable {{{name}}}"),
            Problem::CallRemoved => write!(f, "use {{MYCALL}}"),
            Problem::UnbalancedBrace => write!(f, "unbalanced brace"),
            Problem::UnsendableChar(c) => write!(f, "{c:?} cannot be sent as CW"),
        }
    }
}

/// Something worth a note that does not stop a send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Warning {
    TooLong { len: usize },
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Warning::TooLong { len } => {
                write!(f, "{len} characters when filled in (over {LONG_MESSAGE})")
            }
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    pub problems: Vec<Problem>,
    pub warnings: Vec<Warning>,
}

enum Token {
    Text(String),
    Var(Variable),
}

/// Characters the keyer can send, after uppercasing: letters, digits, space and the punctuation
/// CW has codes for.
fn sendable(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == ' ' || ".,?/=+-:'\"()@!&$".contains(c)
}

fn push_unique(problems: &mut Vec<Problem>, problem: Problem) {
    if !problems.contains(&problem) {
        problems.push(problem);
    }
}

/// Split a message into text and variables, collecting structural problems on the way.
fn tokenize(msg: &str) -> (Vec<Token>, Vec<Problem>) {
    let mut tokens = Vec::new();
    let mut problems = Vec::new();
    let mut text = String::new();
    let mut chars = msg.chars();
    while let Some(c) = chars.next() {
        match c {
            '{' => {
                let mut name = String::new();
                let mut closed = false;
                for inner in chars.by_ref() {
                    match inner {
                        '}' => {
                            closed = true;
                            break;
                        }
                        '{' => break,
                        _ => name.push(inner),
                    }
                }
                if !closed {
                    push_unique(&mut problems, Problem::UnbalancedBrace);
                    continue;
                }
                if !text.is_empty() {
                    tokens.push(Token::Text(std::mem::take(&mut text)));
                }
                if name.eq_ignore_ascii_case("CALL") {
                    push_unique(&mut problems, Problem::CallRemoved);
                } else if let Some(variable) = Variable::parse(&name) {
                    tokens.push(Token::Var(variable));
                } else {
                    push_unique(&mut problems, Problem::UnknownVariable(name.to_uppercase()));
                }
            }
            '}' => push_unique(&mut problems, Problem::UnbalancedBrace),
            _ => {
                for u in c.to_uppercase() {
                    if !sendable(u) {
                        push_unique(&mut problems, Problem::UnsendableChar(c));
                    }
                    text.push(u);
                }
            }
        }
    }
    if !text.is_empty() {
        tokens.push(Token::Text(text));
    }
    (tokens, problems)
}

/// The one validator: rejects unknown variables, `{CALL}`, unbalanced braces and characters the
/// keyer cannot send, and warns past [`LONG_MESSAGE`] characters after worst-case expansion.
pub fn validate(msg: &str) -> Report {
    let (tokens, problems) = tokenize(msg);
    let len: usize = tokens
        .iter()
        .map(|t| match t {
            Token::Text(s) => s.chars().count(),
            Token::Var(v) => v.worst_case_len(),
        })
        .sum();
    let warnings = if problems.is_empty() && len > LONG_MESSAGE {
        vec![Warning::TooLong { len }]
    } else {
        Vec::new()
    };
    Report { problems, warnings }
}

/// Why a message was not expanded into something sendable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendRefusal {
    Invalid(Vec<Problem>),
    Missing(Vec<Variable>),
}

impl fmt::Display for SendRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SendRefusal::Invalid(problems) => {
                let list: Vec<String> = problems.iter().map(ToString::to_string).collect();
                write!(f, "invalid message: {}", list.join(", "))
            }
            SendRefusal::Missing(variables) => {
                let list: Vec<&str> = variables.iter().map(|v| v.name()).collect();
                write!(f, "{} not set", list.join(", "))
            }
        }
    }
}

/// Expand `msg` from live state for sending. The whole send is refused, with the reason, when the
/// message is invalid, a variable has no value, or a value holds a character the keyer cannot
/// send; partial text and literal braces never come out.
pub fn expand(msg: &str, live: &Live) -> Result<String, SendRefusal> {
    let (tokens, problems) = tokenize(msg);
    if !problems.is_empty() {
        return Err(SendRefusal::Invalid(problems));
    }
    let mut missing = Vec::new();
    let mut out = String::new();
    for token in &tokens {
        match token {
            Token::Text(s) => out.push_str(s),
            Token::Var(v) => match live.get(*v) {
                Some(value) => out.push_str(&value.to_uppercase()),
                None if missing.contains(v) => {}
                None => missing.push(*v),
            },
        }
    }
    if !missing.is_empty() {
        return Err(SendRefusal::Missing(missing));
    }
    let mut bad = Vec::new();
    for c in out.chars().filter(|&c| !sendable(c)) {
        push_unique(&mut bad, Problem::UnsendableChar(c));
    }
    if bad.is_empty() {
        Ok(out)
    } else {
        Err(SendRefusal::Invalid(bad))
    }
}

/// `msg` with the variables that have a value filled in and the rest left as written, for
/// showing on the bar. Never sent.
pub fn preview(msg: &str, live: &Live) -> String {
    let (tokens, problems) = tokenize(msg);
    if !problems.is_empty() {
        return msg.to_string();
    }
    tokens
        .iter()
        .map(|t| match t {
            Token::Text(s) => s.clone(),
            Token::Var(v) => live
                .get(*v)
                .map_or_else(|| format!("{{{}}}", v.name()), str::to_uppercase),
        })
        .collect()
}

/// Where a key's message came from, highest layer first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    PlanOverride,
    PlanSet,
    UserSet,
    BuiltIn,
}

impl Layer {
    pub fn label(self) -> &'static str {
        match self {
            Layer::PlanOverride => "plan key",
            Layer::PlanSet => "plan set",
            Layer::UserSet => "your set",
            Layer::BuiltIn => "built-in",
        }
    }
}

/// A named set of messages. `keys: None` means the name does not exist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedSet {
    pub name: String,
    pub keys: Option<BTreeMap<u8, String>>,
}

/// The layers to resolve, top to bottom, before the built-in defaults.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Layers {
    /// Single-key overrides from the plan.
    pub plan_override: BTreeMap<u8, String>,
    /// The plan's named set.
    pub plan_set: Option<NamedSet>,
    /// The user's set for the Activity (`[softkeys.<name>]`, or `[macros]` for General).
    pub user_set: Option<NamedSet>,
}

/// One key after layering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedKey {
    pub text: String,
    pub layer: Layer,
    /// Why it cannot be sent; empty when valid.
    pub problems: Vec<Problem>,
    pub warnings: Vec<Warning>,
}

impl ResolvedKey {
    fn new(text: &str, layer: Layer) -> Self {
        let report = validate(text);
        Self {
            text: text.to_string(),
            layer,
            problems: report.problems,
            warnings: report.warnings,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.problems.is_empty()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Resolved {
    pub keys: BTreeMap<u8, ResolvedKey>,
    /// Named sets that were asked for and do not exist. Flagged, never blocking.
    pub missing_sets: Vec<String>,
}

/// The built-in default messages, the bottom layer.
pub fn built_in() -> BTreeMap<u8, &'static str> {
    BTreeMap::from([
        (1, "CQ CQ DE {MYCALL} {MYCALL} K"),
        (2, "{THEIRCALL} DE {MYCALL} UR {MYRST} {MYRST} K"),
        (3, "{THEIRCALL} TU 73 DE {MYCALL} EE"),
        (4, "{MYCALL}"),
        (5, "?"),
        (6, "AGN"),
    ])
}

/// Layer per key: plan override, the plan's set, the user's set, then the built-in default.
pub fn resolve(layers: &Layers) -> Resolved {
    let mut resolved = Resolved::default();
    let in_range = |n: &u8| (1..=SLOTS).contains(n);
    let sets = [
        (Layer::PlanSet, &layers.plan_set),
        (Layer::UserSet, &layers.user_set),
    ];
    for (_, set) in sets {
        if let Some(NamedSet { name, keys: None }) = set {
            resolved.missing_sets.push(name.clone());
        }
    }
    for (n, text) in built_in() {
        resolved
            .keys
            .insert(n, ResolvedKey::new(text, Layer::BuiltIn));
    }
    for (layer, set) in sets.into_iter().rev() {
        if let Some(NamedSet {
            keys: Some(keys), ..
        }) = set
        {
            for (n, text) in keys.iter().filter(|(n, _)| in_range(n)) {
                resolved.keys.insert(*n, ResolvedKey::new(text, layer));
            }
        }
    }
    for (n, text) in layers.plan_override.iter().filter(|(n, _)| in_range(n)) {
        resolved
            .keys
            .insert(*n, ResolvedKey::new(text, Layer::PlanOverride));
    }
    resolved
}

/// The other values variables expand from, held by the app and set by whatever knows them (the
/// Contact form, the active Activation plan). `MYCALL` comes from the config's callsign.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Exchange {
    pub their_call: Option<String>,
    pub my_ref: Option<String>,
    pub their_ref: Option<String>,
    pub my_rst: Option<String>,
    pub their_rst: Option<String>,
}

impl Exchange {
    pub fn live<'a>(&'a self, my_call: Option<&'a str>) -> Live<'a> {
        Live {
            my_call,
            their_call: self.their_call.as_deref(),
            my_ref: self.my_ref.as_deref(),
            their_ref: self.their_ref.as_deref(),
            my_rst: self.my_rst.as_deref(),
            their_rst: self.their_rst.as_deref(),
        }
    }
}
