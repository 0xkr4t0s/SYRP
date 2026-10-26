//! [`LocalisedStrings`]: a parsed translation file.

use std::collections::HashMap;

use crate::{StringsError, StringsErrorKind};

/// A set of translations for one language, parsed from a translation file.
///
/// # File format
/// One item per line, in any order:
///
/// - `language: French`: the language's name (the rest of the line, trimmed).
/// - `countries: fr be mc ch lu`: country codes, separated by spaces or commas.
/// - `"original text" = "translated text"`: one mapping. Inside the quotes, write `\"` for a
///   quote, `\\` for a backslash, and `\n`, `\r`, `\t` for line feed, carriage return and tab.
///   A `//` or `#` comment may follow the translated string.
/// - Blank lines, and lines starting with `//` or `#`, are ignored.
///
/// Any other line is an error (`SYRP-STRINGS-0004`) carrying its 1-based line number. When an
/// original appears twice, the later mapping wins. A UTF-8 byte-order mark is skipped.
///
/// # Lookup
/// [`LocalisedStrings::translate`] returns the mapped text, else asks the fallback set (see
/// [`LocalisedStrings::with_fallback`]), else returns its input unchanged. With `ignore_case`,
/// originals are compared after Unicode lower-casing.
///
/// ```
/// use syrp_strings::LocalisedStrings;
/// let file = "language: French\ncountries: fr be\n\n\"Hello\" = \"Bonjour\"\n";
/// let fr = LocalisedStrings::parse(file, false).unwrap();
/// assert_eq!(fr.language_name(), "French");
/// assert_eq!(fr.country_codes(), ["fr", "be"]);
/// assert_eq!(fr.translate("Hello"), "Bonjour");
/// assert_eq!(fr.translate("Goodbye"), "Goodbye");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LocalisedStrings {
    language: String,
    countries: Vec<String>,
    mappings: HashMap<String, String>,
    ignore_case: bool,
    fallback: Option<Box<LocalisedStrings>>,
}

impl LocalisedStrings {
    /// Parses a translation file (see the type documentation for the format).
    ///
    /// `ignore_case` makes [`LocalisedStrings::translate`] match originals regardless of case.
    ///
    /// # Errors
    /// `SYRP-STRINGS-0004` ([`StringsErrorKind::TranslationSyntax`]) for a malformed line;
    /// [`StringsError::line`] gives its 1-based number and [`StringsError::position`] the byte
    /// offset within it.
    ///
    /// ```
    /// use syrp_strings::LocalisedStrings;
    /// let de = LocalisedStrings::parse(r#""OK" = "OK" // same in German"#, true).unwrap();
    /// assert_eq!(de.translate("ok"), "OK");
    /// assert_eq!(LocalisedStrings::parse("\"a\" \"b\"", false).unwrap_err().line(), Some(1));
    /// ```
    #[doc(alias = "loadFromText")]
    pub fn parse(text: &str, ignore_case: bool) -> Result<Self, StringsError> {
        let mut out = Self {
            ignore_case,
            ..Self::default()
        };
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        for (index, raw) in text.lines().enumerate() {
            let line = index + 1;
            let indent = raw.len() - raw.trim_start().len();
            let body = raw.trim();
            if body.is_empty() || is_comment(body) {
                continue;
            }
            if let Some(name) = header(body, "language:") {
                out.language = name.trim().to_owned();
            } else if let Some(codes) = header(body, "countries:") {
                out.countries.extend(
                    codes
                        .split([' ', '\t', ','])
                        .filter(|c| !c.is_empty())
                        .map(str::to_owned),
                );
            } else {
                let err = |at: usize| {
                    StringsError::at_line(StringsErrorKind::TranslationSyntax, line, indent + at)
                };
                let (original, after) = quoted(body).map_err(err)?;
                let rest = body[after..].trim_start();
                let Some(rest) = rest.strip_prefix('=') else {
                    return Err(err(body.len() - rest.len()));
                };
                let start = body.len() - rest.trim_start().len();
                let (translated, end) = quoted(&body[start..]).map_err(|at| err(start + at))?;
                let tail = body[start + end..].trim_start();
                if !tail.is_empty() && !is_comment(tail) {
                    return Err(err(body.len() - tail.len()));
                }
                out.insert(original, translated);
            }
        }
        Ok(out)
    }

    /// The language name from the `language:` line, or `""` if there was none.
    ///
    /// ```
    /// let s = syrp_strings::LocalisedStrings::parse("language: Español", false).unwrap();
    /// assert_eq!(s.language_name(), "Español");
    /// ```
    #[doc(alias = "getLanguageName")]
    pub fn language_name(&self) -> &str {
        &self.language
    }

    /// The country codes from the `countries:` line, in file order.
    ///
    /// ```
    /// let s = syrp_strings::LocalisedStrings::parse("countries: de, at ch", false).unwrap();
    /// assert_eq!(s.country_codes(), ["de", "at", "ch"]);
    /// ```
    #[doc(alias = "getCountryCodes")]
    pub fn country_codes(&self) -> &[String] {
        &self.countries
    }

    /// Whether originals are matched regardless of case.
    ///
    /// ```
    /// assert!(syrp_strings::LocalisedStrings::parse("", true).unwrap().ignores_case());
    /// ```
    pub fn ignores_case(&self) -> bool {
        self.ignore_case
    }

    /// The number of mappings in this set, not counting the fallback.
    ///
    /// ```
    /// let s = syrp_strings::LocalisedStrings::parse("\"a\" = \"b\"\n\"a\" = \"c\"", false).unwrap();
    /// assert_eq!(s.len(), 1);
    /// ```
    pub fn len(&self) -> usize {
        self.mappings.len()
    }

    /// Whether this set has no mappings (the fallback is not counted).
    ///
    /// ```
    /// assert!(syrp_strings::LocalisedStrings::default().is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.mappings.is_empty()
    }

    /// Returns the translation of `text`, from this set or its fallback, or `text` unchanged
    /// when neither has one.
    ///
    /// ```
    /// let s = syrp_strings::LocalisedStrings::parse(r#""Save" = "Sichern""#, false).unwrap();
    /// assert_eq!(s.translate("Save"), "Sichern");
    /// assert_eq!(s.translate("save"), "save");
    /// ```
    pub fn translate<'a>(&'a self, text: &'a str) -> &'a str {
        self.get(text).unwrap_or(text)
    }

    /// Returns the translation of `text` from this set or its fallback, if there is one.
    ///
    /// ```
    /// let s = syrp_strings::LocalisedStrings::parse(r#""Yes" = "Oui""#, false).unwrap();
    /// assert_eq!(s.get("Yes"), Some("Oui"));
    /// assert_eq!(s.get("No"), None);
    /// ```
    pub fn get(&self, text: &str) -> Option<&str> {
        let found = if self.ignore_case {
            self.mappings.get(&text.to_lowercase())
        } else {
            self.mappings.get(text)
        };
        match found {
            Some(t) => Some(t),
            None => self.fallback.as_deref().and_then(|f| f.get(text)),
        }
    }

    /// Returns this set with `fallback` consulted for originals it does not map.
    ///
    /// Fallbacks chain: a fallback may have its own fallback. Replaces any earlier fallback.
    ///
    /// ```
    /// use syrp_strings::LocalisedStrings;
    /// let base = LocalisedStrings::parse(r#""Open" = "Ouvrir""#, false).unwrap();
    /// let fr_ca = LocalisedStrings::parse(r#""Email" = "Courriel""#, false).unwrap().with_fallback(base);
    /// assert_eq!((fr_ca.translate("Email"), fr_ca.translate("Open")), ("Courriel", "Ouvrir"));
    /// ```
    #[doc(alias = "setFallback")]
    pub fn with_fallback(mut self, fallback: LocalisedStrings) -> Self {
        self.fallback = Some(Box::new(fallback));
        self
    }

    /// The fallback set, if one was given.
    ///
    /// ```
    /// assert!(syrp_strings::LocalisedStrings::default().fallback().is_none());
    /// ```
    pub fn fallback(&self) -> Option<&LocalisedStrings> {
        self.fallback.as_deref()
    }

    /// Adds `other`'s mappings to this set; where both map the same original, `other` wins.
    ///
    /// Country codes not already present are appended. If this set has no language name it
    /// takes `other`'s. `other`'s fallback is not merged.
    ///
    /// # Errors
    /// `SYRP-STRINGS-0005` ([`StringsErrorKind::LanguageMismatch`]) when both sets name a
    /// language and the names differ ignoring case; this set is then unchanged.
    ///
    /// ```
    /// use syrp_strings::LocalisedStrings;
    /// let mut a = LocalisedStrings::parse("language: French\n\"a\" = \"1\"", false).unwrap();
    /// let b = LocalisedStrings::parse("language: french\n\"b\" = \"2\"", false).unwrap();
    /// a.merge(&b).unwrap();
    /// assert_eq!((a.translate("a"), a.translate("b")), ("1", "2"));
    /// ```
    #[doc(alias = "addStrings")]
    pub fn merge(&mut self, other: &LocalisedStrings) -> Result<(), StringsError> {
        let Self {
            language,
            countries,
            mappings,
            ignore_case: _,
            fallback: _,
        } = other;
        if !self.language.is_empty()
            && !language.is_empty()
            && self.language.to_lowercase() != language.to_lowercase()
        {
            return Err(StringsError::new(StringsErrorKind::LanguageMismatch, None));
        }
        if self.language.is_empty() {
            self.language.clone_from(language);
        }
        for code in countries {
            if !self.countries.contains(code) {
                self.countries.push(code.clone());
            }
        }
        for (original, translated) in mappings {
            self.insert(original.clone(), translated.clone());
        }
        Ok(())
    }

    fn insert(&mut self, original: String, translated: String) {
        let key = if self.ignore_case {
            original.to_lowercase()
        } else {
            original
        };
        self.mappings.insert(key, translated);
    }
}

fn is_comment(text: &str) -> bool {
    text.starts_with("//") || text.starts_with('#')
}

/// The rest of `line` after `name`, matched ignoring ASCII case.
fn header<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    let head = line.get(..name.len())?;
    head.eq_ignore_ascii_case(name).then(|| &line[name.len()..])
}

/// Reads a quoted string at the start of `text`. Returns the unescaped text and the byte offset
/// just past the closing quote, or the byte offset of the problem.
fn quoted(text: &str) -> Result<(String, usize), usize> {
    let mut chars = text.char_indices();
    if !matches!(chars.next(), Some((_, '"'))) {
        return Err(0);
    }
    let mut out = String::new();
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => return Ok((out, i + 1)),
            '\\' => out.push(match chars.next() {
                Some((_, '"')) => '"',
                Some((_, '\\')) => '\\',
                Some((_, 'n')) => '\n',
                Some((_, 'r')) => '\r',
                Some((_, 't')) => '\t',
                _ => return Err(i),
            }),
            c => out.push(c),
        }
    }
    Err(text.len())
}
