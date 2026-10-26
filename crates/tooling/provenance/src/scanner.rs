// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Tokenising, normalising and winnowing (B2).
//!
//! The tokeniser understands C-family lexical structure for a [`Dialect`] (Rust or C++), so
//! a C++ reference corpus and Rust files under test normalise to comparable tokens: comments are
//! dropped, identifiers and keywords are lowercased, and literals collapse into buckets
//! (`<str>`, `<chr>`, `<int>`, `<flt>`). Consecutive runs of `k` normalised tokens are hashed
//! with FNV-1a 64, and winnowing with window `w` keeps the rightmost minimum hash of each
//! window. Any two texts sharing a run of at least `k + w - 1` normalised tokens share a
//! fingerprint.
//!
//! ```
//! use syrp_provenance::scanner::{Dialect, fingerprints};
//! let a = fingerprints("let Total = 1 + 2; // hello", 3, 2, Dialect::Rust);
//! let b = fingerprints("let total = 7 + 9;", 3, 2, Dialect::Rust);
//! assert_eq!(a.iter().map(|f| f.hash).collect::<Vec<_>>(),
//!            b.iter().map(|f| f.hash).collect::<Vec<_>>());
//! ```

/// Identifies the normalisation rules. Fingerprint databases record it; bump it whenever
/// [`tokenize`] or [`fingerprints`] would produce different hashes for the same input.
pub const NORMALISER: &str = "syrp-cfamily-v2";

/// The source language, which decides the few lexical rules that differ.
///
/// ```
/// use syrp_provenance::scanner::Dialect;
/// assert_eq!(Dialect::for_path("src/lib.rs"), Dialect::Rust);
/// assert_eq!(Dialect::for_path("juce_core/text/juce_String.cpp"), Dialect::Cpp);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    /// Rust: block comments nest.
    Rust,
    /// C and C++: block comments end at the first `*/`.
    Cpp,
}

impl Dialect {
    /// [`Dialect::Rust`] for `.rs` files, [`Dialect::Cpp`] for everything else.
    ///
    /// ```
    /// use syrp_provenance::scanner::Dialect;
    /// assert_eq!(Dialect::for_path("a.h"), Dialect::Cpp);
    /// ```
    pub fn for_path(path: &str) -> Self {
        if path.ends_with(".rs") {
            Dialect::Rust
        } else {
            Dialect::Cpp
        }
    }
}

/// One normalised token and the 1-based line it starts on.
///
/// ```
/// use syrp_provenance::scanner::{Dialect, tokenize};
/// let t = &tokenize("Foo", Dialect::Rust)[0];
/// assert_eq!((t.text.as_str(), t.line), ("foo", 1));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// Normalised text: a lowercased identifier, a literal bucket or a punctuation character.
    pub text: String,
    /// 1-based source line.
    pub line: u32,
}

/// One selected fingerprint.
///
/// ```
/// use syrp_provenance::scanner::{Dialect, fingerprints};
/// let f = &fingerprints("a b c", 3, 1, Dialect::Rust)[0];
/// assert_eq!((f.first_line, f.last_line), (1, 1));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fingerprint {
    /// FNV-1a 64 hash of the n-gram.
    pub hash: u64,
    /// Line of the n-gram's first token.
    pub first_line: u32,
    /// Line of the n-gram's last token.
    pub last_line: u32,
}

const INT_SUFFIXES: &[&str] = &[
    "usize", "isize", "u128", "i128", "u64", "i64", "u32", "i32", "u16", "i16", "u8", "i8",
];

fn is_ident_start(c: char) -> bool {
    c == '_' || c.is_alphabetic()
}

fn is_ident_char(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

struct Lexer {
    dialect: Dialect,
    chars: Vec<char>,
    pos: usize,
    line: u32,
}

impl Lexer {
    fn peek(&self, ahead: usize) -> Option<char> {
        self.chars.get(self.pos + ahead).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek(0)?;
        self.pos += 1;
        if c == '\n' {
            self.line += 1;
        }
        Some(c)
    }

    /// Consumes a quoted literal whose opening quote is at the cursor.
    fn quoted(&mut self, quote: char) {
        self.bump();
        while let Some(c) = self.bump() {
            match c {
                '\\' => {
                    self.bump();
                }
                c if c == quote => return,
                _ => {}
            }
        }
    }

    /// Consumes a raw string starting at the `#`s or `"` after its prefix.
    fn raw_string(&mut self) {
        let mut hashes = 0;
        while self.peek(0) == Some('#') {
            self.bump();
            hashes += 1;
        }
        self.bump(); // opening quote
        while let Some(c) = self.bump() {
            if c == '"' && (0..hashes).all(|i| self.peek(i) == Some('#')) {
                for _ in 0..hashes {
                    self.bump();
                }
                return;
            }
        }
    }

    fn block_comment(&mut self) {
        self.bump();
        self.bump();
        let mut depth = 1;
        while depth > 0 {
            match (self.bump(), self.peek(0)) {
                (None, _) => return,
                (Some('/'), Some('*')) if self.dialect == Dialect::Rust => {
                    self.bump();
                    depth += 1;
                }
                (Some('*'), Some('/')) => {
                    self.bump();
                    depth -= 1;
                }
                _ => {}
            }
        }
    }

    /// `after_dot` stops `x.0.1` (tuple fields) from reading `0.1` as a float.
    fn number(&mut self, after_dot: bool) -> &'static str {
        let hex = self.peek(0) == Some('0') && matches!(self.peek(1), Some('x' | 'X'));
        let mut float = false;
        let mut text = String::new();
        while let Some(c) = self.peek(0) {
            if is_ident_char(c) {
                if !hex && matches!(c, 'e' | 'E') && matches!(self.peek(1), Some('+' | '-')) {
                    self.bump();
                    float = true;
                }
                text.push(c);
            } else if c == '.'
                && !float
                && !after_dot
                && self.peek(1).is_some_and(|n| n.is_ascii_digit())
            {
                float = true;
            } else {
                break;
            }
            self.bump();
        }
        let body = INT_SUFFIXES
            .iter()
            .find_map(|s| text.strip_suffix(s))
            .unwrap_or(&text);
        if !hex && (body.contains(['e', 'E']) || body.ends_with("f32") || body.ends_with("f64")) {
            float = true;
        }
        if float { "<flt>" } else { "<int>" }
    }
}

/// Splits `source` into normalised tokens, dropping whitespace and comments.
///
/// ```
/// use syrp_provenance::scanner::{Dialect, tokenize};
/// let texts: Vec<_> = tokenize("Gain = 0.5; /* c */ name(\"x\", 'y', 42)", Dialect::Rust)
///     .into_iter().map(|t| t.text).collect();
/// assert_eq!(texts, ["gain", "=", "<flt>", ";", "name", "(", "<str>", ",", "<chr>", ",",
///                    "<int>", ")"]);
/// ```
pub fn tokenize(source: &str, dialect: Dialect) -> Vec<Token> {
    let mut lx = Lexer {
        dialect,
        chars: source.chars().collect(),
        pos: 0,
        line: 1,
    };
    let mut out = Vec::new();
    while let Some(c) = lx.peek(0) {
        let line = lx.line;
        let text: String = if c.is_whitespace() {
            lx.bump();
            continue;
        } else if c == '/' && lx.peek(1) == Some('/') {
            while lx.peek(0).is_some_and(|c| c != '\n') {
                lx.bump();
            }
            continue;
        } else if c == '/' && lx.peek(1) == Some('*') {
            lx.block_comment();
            continue;
        } else if c == '"' {
            lx.quoted('"');
            "<str>".into()
        } else if c == '\'' {
            if lx.peek(1) == Some('\\') || lx.peek(2) == Some('\'') {
                lx.quoted('\'');
                "<chr>".into()
            } else {
                lx.bump();
                let mut s = String::from("'");
                while let Some(c) = lx.peek(0).filter(|c| is_ident_char(*c)) {
                    s.extend(c.to_lowercase());
                    lx.bump();
                }
                s
            }
        } else if c.is_ascii_digit() {
            lx.number(out.last().is_some_and(|t: &Token| t.text == "."))
                .into()
        } else if is_ident_start(c) {
            let mut word = String::new();
            while let Some(c) = lx.peek(0).filter(|c| is_ident_char(*c)) {
                word.push(c);
                lx.bump();
            }
            let next = lx.peek(0);
            match (word.as_str(), next) {
                ("r", Some('#')) if lx.peek(1).is_some_and(is_ident_start) => {
                    lx.bump(); // raw identifier: drop the prefix
                    continue;
                }
                ("r" | "br" | "cr", Some('#' | '"')) => {
                    lx.raw_string();
                    "<str>".into()
                }
                ("b" | "c", Some('"')) => {
                    lx.quoted('"');
                    "<str>".into()
                }
                ("b", Some('\'')) => {
                    lx.quoted('\'');
                    "<chr>".into()
                }
                _ => word.to_lowercase(),
            }
        } else {
            lx.bump();
            c.to_string()
        };
        out.push(Token { text, line });
    }
    out
}

/// FNV-1a 64 over the token texts, each followed by a 0x1f separator byte.
fn hash_gram(tokens: &[Token]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for t in tokens {
        for b in t.text.bytes().chain(std::iter::once(0x1f)) {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    h
}

/// Hashes every `k`-gram of tokens and winnows them with window `w` (both clamped to at
/// least 1). Text with fewer than `k` tokens has no fingerprints.
///
/// ```
/// use syrp_provenance::scanner::{Dialect, fingerprints};
/// assert!(fingerprints("too short", 5, 4, Dialect::Rust).is_empty());
/// assert!(!fingerprints("a b c d e f g h", 5, 4, Dialect::Rust).is_empty());
/// ```
pub fn fingerprints(source: &str, k: usize, w: usize, dialect: Dialect) -> Vec<Fingerprint> {
    let (k, w) = (k.max(1), w.max(1));
    let tokens = tokenize(source, dialect);
    if tokens.len() < k {
        return Vec::new();
    }
    let grams: Vec<Fingerprint> = tokens
        .windows(k)
        .map(|g| Fingerprint {
            hash: hash_gram(g),
            first_line: g[0].line,
            last_line: g[k - 1].line,
        })
        .collect();
    let mut out = Vec::new();
    let mut last_pick = None;
    let windows = grams.len().saturating_sub(w) + 1;
    for start in 0..windows {
        let end = (start + w).min(grams.len());
        let mut pick = start;
        for i in start..end {
            if grams[i].hash <= grams[pick].hash {
                pick = i;
            }
        }
        if last_pick != Some(pick) {
            out.push(grams[pick]);
            last_pick = Some(pick);
        }
    }
    out
}
