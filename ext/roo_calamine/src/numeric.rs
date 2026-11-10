//! roo's text-to-number conversions, for the plainly written numbers that
//! worksheets store: `-?\d+(\.\d+)?([eE][-+]?\d+)?`. Any other text gives
//! `None`, and the caller leaves the conversion to roo's own code.

use magnus::{IntoValue, Ruby, Value};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Number {
    Integer(i64),
    Float(f64),
}

impl IntoValue for Number {
    fn into_value_with(self, ruby: &Ruby) -> Value {
        match self {
            Number::Integer(integer) => ruby.integer_from_i64(integer).into_value_with(ruby),
            Number::Float(float) => ruby.float_from_f64(float).into_value_with(ruby),
        }
    }
}

/// Plain number text, split where Ruby's conversions tell it apart.
struct Plain<'a> {
    /// The sign and the digits before any `.` or exponent.
    integer: &'a str,
    fraction: bool,
    exponent: bool,
    /// The digits before the exponent.
    mantissa: &'a [u8],
}

impl<'a> Plain<'a> {
    fn parse(text: &'a str) -> Option<Self> {
        let bytes = text.as_bytes();
        let mut at = usize::from(bytes.first() == Some(&b'-'));
        let digits = |at: &mut usize| {
            let start = *at;
            while bytes.get(*at).is_some_and(u8::is_ascii_digit) {
                *at += 1;
            }
            *at > start
        };
        if !digits(&mut at) {
            return None;
        }
        let integer_end = at;
        let fraction = bytes.get(at) == Some(&b'.');
        if fraction {
            at += 1;
            if !digits(&mut at) {
                return None;
            }
        }
        let mantissa_end = at;
        let exponent = matches!(bytes.get(at), Some(b'e' | b'E'));
        if exponent {
            at += 1;
            if matches!(bytes.get(at), Some(b'+' | b'-')) {
                at += 1;
            }
            if !digits(&mut at) {
                return None;
            }
        }
        (at == bytes.len()).then(|| Self {
            integer: &text[..integer_end],
            fraction,
            exponent,
            mantissa: &bytes[..mantissa_end],
        })
    }

    /// The integer part, when it fits an `i64`.
    fn integer(&self) -> Option<i64> {
        self.integer.parse().ok()
    }
}

/// `Cell::Number#create_numeric(text)` under a format that matches `/%/` or
/// `/\.0/` (`always_float`) or one that does not.
pub fn create_numeric(text: &str, always_float: bool) -> Option<Number> {
    let plain = Plain::parse(text)?;
    if always_float || plain.fraction || plain.exponent {
        // Ruby's `strtod` stops reading fraction digits after about 60 digits,
        // so a longer mantissa can round differently.
        if plain.mantissa.len() > 40 {
            return None;
        }
        let float: f64 = text.parse().ok()?;
        // Ruby's `Float()` warns when the text is out of range (infinite, or
        // zero although a digit is not); subnormal results are left to Ruby
        // too.
        let nonzero = plain.mantissa.iter().any(|b| matches!(b, b'1'..=b'9'));
        let in_range = float.is_normal() || (float == 0.0 && !nonzero);
        in_range.then_some(Number::Float(float))
    } else {
        plain.integer().map(Number::Integer)
    }
}

/// Text that `Float()` and `Integer(text, 10)` both read as a finite number,
/// and `String#to_f` reads whole.
pub fn plain_number(text: &str) -> bool {
    Plain::parse(text).is_some() && text.parse::<f64>().is_ok_and(f64::is_finite)
}

/// `String#to_i` of plain number text.
pub fn leading_integer(text: &str) -> Option<i64> {
    Plain::parse(text)?.integer()
}

#[cfg(test)]
mod tests {
    use super::{create_numeric, leading_integer, Number};

    #[test]
    fn integers_and_floats_follow_the_text_and_the_format() {
        assert_eq!(create_numeric("12", false), Some(Number::Integer(12)));
        assert_eq!(create_numeric("-0", false), Some(Number::Integer(0)));
        assert_eq!(create_numeric("007", false), Some(Number::Integer(7)));
        assert_eq!(create_numeric("12", true), Some(Number::Float(12.0)));
        assert_eq!(create_numeric("1.5", false), Some(Number::Float(1.5)));
        assert_eq!(create_numeric("1E3", false), Some(Number::Float(1000.0)));
        assert_eq!(create_numeric("2.5e-3", false), Some(Number::Float(0.0025)));
        let Some(Number::Float(negative_zero)) = create_numeric("-0", true) else {
            panic!("-0 under a float format is a Float");
        };
        assert!(negative_zero.is_sign_negative());
        assert_eq!(create_numeric("0e999", false), Some(Number::Float(0.0)));
    }

    #[test]
    fn leaves_other_text_to_roo() {
        for text in [
            "",
            "-",
            "+5",
            " 1",
            "1 ",
            "1_000",
            "0x1A",
            "1.",
            ".5",
            "1.e5",
            "1e",
            "1e+",
            "inf",
            "NaN",
            "#N/A",
            "1e400",
            "1e-400",
            "4.9e-324",
            "12855504354071923631583389444689181878463593399757479065157632.5",
        ] {
            assert_eq!(create_numeric(text, false), None, "{text:?}");
            assert_eq!(create_numeric(text, true), None, "{text:?}");
        }
        assert_eq!(create_numeric("9223372036854775808", false), None);
        assert_eq!(
            create_numeric("-9223372036854775808", false),
            Some(Number::Integer(i64::MIN))
        );
    }

    #[test]
    fn leading_integer_is_string_to_i() {
        assert_eq!(leading_integer("45000"), Some(45000));
        assert_eq!(leading_integer("45000.9"), Some(45000));
        assert_eq!(leading_integer("1E3"), Some(1));
        assert_eq!(leading_integer("-0.5"), Some(0));
        assert_eq!(leading_integer("#N/A"), None);
        assert_eq!(leading_integer("12345678901234567890"), None);
    }
}
