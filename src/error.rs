use std::fmt::{self, Display};
use std::str::Utf8Error;

use serde::{de, ser};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Debug, PartialEq)]
pub enum Error {
    // Variants created via the `ser::Error` and `de::Error` traits.

    Message(String),

    // Variants created directly by Serializer and Deserializer (format specific).

    Eof,
    ExpectedBoolean,
    ExpectedInteger,
    ExpectedDouble,
    ExpectedString,
    NumberTooLarge,
    InvalidUtf8(Utf8Error),
    TrailingBytes(usize),
    Unimplemented(u8),
}

impl ser::Error for Error {
    fn custom<T: Display>(msg: T) -> Self {
        Error::Message(msg.to_string())
    }
}

impl de::Error for Error {
    fn custom<T: Display>(msg: T) -> Self {
        Error::Message(msg.to_string())
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::Message(ref msg) => write!(f, "{}", msg),
            Error::Eof => write!(f, "unexpected end of input"),
            Error::ExpectedBoolean => write!(f, "expected boolean value in input"),
            Error::ExpectedInteger => write!(f, "expected integer value in input"),
            Error::ExpectedDouble => write!(f,"expected double value in input"),
            Error::ExpectedString => write!(f, "expected string value in input"),
            Error::NumberTooLarge => write!(f, "number was too large to parse into requested type"),
            Error::InvalidUtf8(_utf8err) => write!(f, "invalid utf8 encountered when parsing string"),
            Error::TrailingBytes(length) => write!(f, "found {} trailing bytes after parsing input", length),
            Error::Unimplemented(b) => write!(f, "parsing for byte sequence starting 0x{:02x} is not implemented", b),
        }
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod tests {
    use std::error::Error;

    #[test]
    fn error() {
        assert_eq!(&format!("{}", crate::error::Error::Message("foo".to_owned())), "foo");
    }
}
