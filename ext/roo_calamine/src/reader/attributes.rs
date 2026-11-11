//! The attributes of a start tag, read in one pass.
//!
//! quick-xml leaves most of an attribute list unchecked, so this reads it
//! along the XML grammar `(S Name S? '=' S? AttValue)* S?` and rejects
//! everything else, as libxml2 does: a missing `=` or quote, an unquoted
//! value, `<` in a value, attributes without whitespace between them, and a
//! name used twice.

use std::borrow::Cow;
use std::collections::HashSet;
use std::ops::Range;

use quick_xml::name::QName;
use quick_xml::XmlVersion;

use super::{check_chars, check_name, Result, Unsupported};

/// An attribute as written, before references are resolved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Attribute<'a> {
    pub key: &'a str,
    pub value: &'a str,
}

impl<'a> Attribute<'a> {
    /// The value with references resolved and whitespace normalized, as
    /// libxml2 reads it. Fails on a reference XML does not define.
    pub fn normalized_value(&self) -> Result<Cow<'a, str>> {
        if !self
            .value
            .bytes()
            .any(|b| matches!(b, b'&' | b'\t' | b'\n' | b'\r'))
        {
            return Ok(Cow::Borrowed(self.value));
        }
        let attribute = quick_xml::events::attributes::Attribute {
            key: QName(self.key),
            value: Cow::Borrowed(self.value),
        };
        Ok(attribute.normalized_value(XmlVersion::Implicit1_0)?)
    }
}

/// The attributes of the last start tag read, as byte ranges of its
/// attribute list, so a tag is read once. The buffer is reused from tag to
/// tag.
#[derive(Default)]
pub struct TagAttributes {
    spans: Vec<(Range<usize>, Range<usize>)>,
    prefixed: bool,
    references: bool,
}

impl TagAttributes {
    /// Reads and checks `raw`, what quick-xml's `BytesStart::attributes_raw`
    /// returns, in place of the tag read before.
    pub fn read(&mut self, raw: &str) -> Result<()> {
        self.prefixed = false;
        self.references = false;
        if self.read_common(raw) {
            return Ok(());
        }
        self.spans.clear();
        check_chars(raw)?;
        let bytes = raw.as_bytes();
        let mut position = 0;
        loop {
            let start = skip_whitespace(bytes, position);
            if start == bytes.len() {
                break;
            }
            if start == position {
                return Err(Unsupported(
                    "attributes without whitespace between them".into(),
                ));
            }
            let key_end = start
                + bytes[start..]
                    .iter()
                    .position(|&b| b == b'=' || is_whitespace(b))
                    .unwrap_or(bytes.len() - start);
            let key = &raw[start..key_end];
            check_name(key)?;
            self.prefixed |= key.contains(':');
            let equals = skip_whitespace(bytes, key_end);
            if bytes.get(equals) != Some(&b'=') {
                return Err(Unsupported(format!("no `=` after the attribute {key}")));
            }
            let quote_at = skip_whitespace(bytes, equals + 1);
            let quote = match bytes.get(quote_at) {
                Some(&quote @ (b'"' | b'\'')) => quote,
                _ => return Err(Unsupported(format!("an unquoted value for {key}"))),
            };
            let value_start = quote_at + 1;
            let mut value_end = value_start;
            loop {
                match bytes.get(value_end) {
                    Some(&b) if b == quote => break,
                    Some(b'<') => return Err(Unsupported("`<` in an attribute value".into())),
                    Some(b'&') => self.references = true,
                    Some(_) => {}
                    None => return Err(Unsupported(format!("an unclosed value for {key}"))),
                }
                value_end += 1;
            }
            self.spans.push((start..key_end, value_start..value_end));
            position = value_end + 1;
        }
        self.check_unique(bytes)
    }

    fn read_common(&mut self, raw: &str) -> bool {
        self.spans.clear();
        let bytes = raw.as_bytes();
        let mut seen = 0u8;
        let mut position = 0;
        while position < bytes.len() {
            let &[b' ', key, b'=', b'"', ..] = &bytes[position..] else {
                return false;
            };
            let bit = match key {
                b'r' => 1,
                b's' => 2,
                b't' => 4,
                _ => return false,
            };
            if seen & bit != 0 {
                return false;
            }
            seen |= bit;
            let value_start = position + 4;
            let Some(length) = bytes[value_start..].iter().position(|&b| b == b'"') else {
                return false;
            };
            let value_end = value_start + length;
            if !bytes[value_start..value_end]
                .iter()
                .all(|&b| matches!(b, b' '..=b'~') && b != b'&' && b != b'<')
            {
                return false;
            }
            self.spans
                .push((position + 1..position + 2, value_start..value_end));
            position = value_end + 1;
        }
        true
    }

    /// Whether a key has a namespace prefix.
    pub fn prefixed(&self) -> bool {
        self.prefixed
    }

    /// Whether a value holds a reference.
    pub fn references(&self) -> bool {
        self.references
    }

    /// The attributes [`read`](Self::read) found in `raw`.
    pub fn iter<'a>(&'a self, raw: &'a str) -> impl Iterator<Item = Attribute<'a>> + 'a {
        self.spans.iter().map(move |(key, value)| Attribute {
            key: &raw[key.clone()],
            value: &raw[value.clone()],
        })
    }

    fn check_unique(&self, bytes: &[u8]) -> Result<()> {
        let key = |(key, _): &(Range<usize>, Range<usize>)| &bytes[key.clone()];
        let unique = if self.spans.len() <= 8 {
            self.spans.iter().enumerate().all(|(i, span)| {
                let key = key(span);
                self.spans[..i]
                    .iter()
                    .all(|earlier| bytes[earlier.0.clone()] != *key)
            })
        } else {
            let mut seen = HashSet::with_capacity(self.spans.len());
            self.spans.iter().all(|span| seen.insert(key(span)))
        };
        if unique {
            Ok(())
        } else {
            Err(Unsupported("an attribute named twice".into()))
        }
    }
}

fn is_whitespace(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn skip_whitespace(bytes: &[u8], from: usize) -> usize {
    from + bytes[from..]
        .iter()
        .take_while(|&&b| is_whitespace(b))
        .count()
}
