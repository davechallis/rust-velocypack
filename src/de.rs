use serde::Deserialize;
use serde::de::{
    self, DeserializeSeed, EnumAccess, IntoDeserializer, MapAccess, SeqAccess,
    VariantAccess, Visitor,
};

use crate::error::{Error, Result};
use std::convert::TryFrom;

pub struct Deserializer<'de> {
    input: &'de [u8],
}

impl<'de> Deserializer<'de> {
    pub fn from_bytes(input: &'de [u8]) -> Self {
        Self { input }
    }

    fn peek_byte(&mut self) -> Result<u8> {
        match self.input.get(0) {
            Some(b) => Ok(*b),
            None => Err(Error::Message("eof".to_owned())),
        }
    }

    fn next_byte(&mut self) -> Result<u8> {
        let b = self.peek_byte()?;
        self.consume_bytes(1);
        Ok(b)
    }

    fn consume_bytes(&mut self, n: usize) {
        self.input = &self.input[n..]
    }

    fn parse_bool(&mut self) -> Result<bool> {
        match self.peek_byte()? {
            0x19 =>  {
                self.consume_bytes(1);
                Ok(false)
            },
            0x1a => {
                self.consume_bytes(1);
                Ok(true)
            },
            _   => Err(Error::Message("ExpectedBoolean".to_owned()))
        }
    }

    fn parse_double(&mut self) -> Result<f64> {
        match self.peek_byte()? {
            0x1b => self.consume_bytes(1),
            _    => return Err(Error::Message("ExpectedDouble".to_owned()))
        }

        let mut bytes: [u8; 8] = Default::default();
        bytes.copy_from_slice(&self.input[..8]);

        let v = f64::from_bits(u64::from_le_bytes(bytes));
        self.consume_bytes(8);
        Ok(v)
    }

    fn parse_signed<T: TryFrom<i64> + TryFrom<u64>>(&mut self) -> Result<T> {
        match self.peek_byte()? {
            b if b >= 0x3a && b <= 0x3f => {
                self.consume_bytes(1);
                Ok(T::try_from(-(0x40 - (b as i64))).unwrap_or_else(|_| panic!("Unable to convert to signed")))
            },
            b if b >= 0x20 && b <= 0x27 => {
                let n_bytes = (b - 0x1f) as usize;
                self.consume_bytes(1);

                let mut le_bytes: [u8; 8] = [0xff; 8];
                le_bytes[..n_bytes].copy_from_slice(&self.input[..n_bytes]);
                let v = match T::try_from(i64::from_le_bytes(le_bytes)) {
                    Ok(v) => v,
                    Err(_) => return Err(Error::Message("NumberTooLarge".to_owned())),
                };
                self.consume_bytes(n_bytes); // number of bytes header plus bytes
                Ok(v)
            },
            b => {
                // else parse into a u64, then attempt to fit into current signed type
                let v_u64: u64 = self.parse_unsigned()?;
                T::try_from(v_u64).map_err(|_| Error::Message("NumberTooLarge".to_owned()))
            }
        }
    }

    fn parse_unsigned<T: TryFrom<u64>>(&mut self) -> Result<T> {
        match self.peek_byte()? {
            b if b >= 0x28 && b <= 0x2f => {
                let n_bytes = (b - 0x27) as usize;
                self.consume_bytes(1);

                let mut le_bytes: [u8; 8] = [0; 8];
                le_bytes[..n_bytes].copy_from_slice(&self.input[..n_bytes]);
                let v = match T::try_from(u64::from_le_bytes(le_bytes)) {
                    Ok(v) => v,
                    Err(_) => return Err(Error::Message("NumberTooLarge".to_owned())),
                };
                self.consume_bytes(n_bytes); // number of bytes header plus bytes
                Ok(v)
            },
            b if b >= 0x30 && b <= 0x39 => {
                let v = match T::try_from((b - 0x30) as u64) {
                    Ok(v) => v,
                    Err(_) => return Err(Error::Message("NumberTooLarge".to_owned())),
                };
                self.consume_bytes(1);
                Ok(v)
            },
            _ => Err(Error::Message("ExpectedInteger".to_owned())),
        }
    }

    fn parse_string(&mut self) -> Result<String> {
        match self.peek_byte()? {
            0xbf => {
                self.consume_bytes(1);
                let mut le_bytes: [u8; 8] = [0; 8];
                le_bytes[..8].copy_from_slice(&self.input[..8]);
                let length = u64::from_le_bytes(le_bytes) as usize;
                self.consume_bytes(8);
                match std::str::from_utf8(&self.input[..length]) {
                    Ok(s) => {
                        self.consume_bytes(length);
                        Ok(s.to_owned())
                    },
                    Err(e) => Err(Error::Message("InvalidUtf8".to_owned()))
                }
            },
            b if b >= 0x40 && b <= 0xbe => {
                self.consume_bytes(1);
                let length = (b - 0x40) as usize;
                if length == 0 {
                    return Ok(String::new())
                }

                match std::str::from_utf8(&self.input[..length]) {
                    Ok(s) => {
                        self.consume_bytes(length);
                        Ok(s.to_owned())
                    },
                    Err(e) => Err(Error::Message("InvalidUtf8".to_owned()))
                }
            },
            _ => Err(Error::Message("ExpectedString".to_owned())),
        }
    }
}

pub fn from_bytes<'a, T: Deserialize<'a>>(s: &'a [u8]) -> Result<T> {
    let mut deserializer = Deserializer::from_bytes(s);
    let t = T::deserialize(&mut deserializer)?;
    if deserializer.input.is_empty() {
        Ok(t)
    } else {
        Err(Error::Message("trailing bytes".to_owned()))
    }
}

impl<'de> Deserializer<'de> {

}

impl<'de, 'a> de::Deserializer<'de> for &'a mut Deserializer<'de> {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value> where
        V: Visitor<'de> {
        match self.peek_byte()? {
            0x18 => self.deserialize_unit(visitor),
            0x19 | 0x1a => self.deserialize_bool(visitor),
            0x1b => self.deserialize_f64(visitor),
            x if (x >= 0x20 && x <= 0x27) || (x >= 0x3a && x <= 0x3f) => self.deserialize_i64(visitor),
            x if x >= 0x28 && x <= 0x39 => self.deserialize_u64(visitor),
            x if x >= 0x40 && x <= 0xbf => self.deserialize_string(visitor),
            _ => Err(Error::Message("unimplemented".to_owned()))
        }
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value> where
        V: Visitor<'de> {
        visitor.visit_bool(self.parse_bool()?)
    }

    fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value> where
        V: Visitor<'de> {
        visitor.visit_i8(self.parse_signed()?)
    }

    fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value> where
        V: Visitor<'de> {
        visitor.visit_i16(self.parse_signed()?)
    }

    fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value> where
        V: Visitor<'de> {
        visitor.visit_i32(self.parse_signed()?)
    }

    fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value> where
        V: Visitor<'de> {
        visitor.visit_i64(self.parse_signed()?)
    }

    fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value> where
        V: Visitor<'de> {
        visitor.visit_u8(self.parse_unsigned()?)
    }

    fn deserialize_u16<V>(self, visitor: V) -> Result<V::Value> where
        V: Visitor<'de> {
        visitor.visit_u16(self.parse_unsigned()?)
    }

    fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value> where
        V: Visitor<'de> {
        visitor.visit_u32(self.parse_unsigned()?)
    }

    fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value> where
        V: Visitor<'de> {
        visitor.visit_u64(self.parse_unsigned()?)
    }

    fn deserialize_f32<V>(self, visitor: V) -> Result<V::Value> where
        V: Visitor<'de> {
        visitor.visit_f32(self.parse_double()? as f32)
    }

    fn deserialize_f64<V>(self, visitor: V) -> Result<V::Value> where
        V: Visitor<'de> {
        visitor.visit_f64(self.parse_double()?)
    }

    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value> where
        V: Visitor<'de> {
        unimplemented!()
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value> where
        V: Visitor<'de> {
        unimplemented!()
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value> where
        V: Visitor<'de> {
        visitor.visit_string(self.parse_string()?)
    }

    fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value> where
        V: Visitor<'de> {
        unimplemented!()
    }

    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value> where
        V: Visitor<'de> {
        unimplemented!()
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value> where
        V: Visitor<'de> {
        unimplemented!()
    }

    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value> where
        V: Visitor<'de> {
        match self.peek_byte()? {
            0x18 => {
                self.consume_bytes(1);
                visitor.visit_unit()
            },
            _    => Err(Error::Message("ExpectedNull".to_owned()))
        }
    }

    fn deserialize_unit_struct<V>(self, _name: &'static str, visitor: V) -> Result<V::Value> where
        V: Visitor<'de> {
        self.deserialize_unit(visitor)
    }

    fn deserialize_newtype_struct<V>(self, name: &'static str, visitor: V) -> Result<V::Value> where
        V: Visitor<'de> {
        unimplemented!()
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value> where
        V: Visitor<'de> {
        unimplemented!()
    }

    fn deserialize_tuple<V>(self, len: usize, visitor: V) -> Result<V::Value> where
        V: Visitor<'de> {
        unimplemented!()
    }

    fn deserialize_tuple_struct<V>(self, name: &'static str, len: usize, visitor: V) -> Result<V::Value> where
        V: Visitor<'de> {
        unimplemented!()
    }

    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value> where
        V: Visitor<'de> {
        unimplemented!()
    }

    fn deserialize_struct<V>(self, name: &'static str, fields: &'static [&'static str], visitor: V) -> Result<V::Value> where
        V: Visitor<'de> {
        unimplemented!()
    }

    fn deserialize_enum<V>(self, name: &'static str, variants: &'static [&'static str], visitor: V) -> Result<V::Value> where
        V: Visitor<'de> {
        unimplemented!()
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value> where
        V: Visitor<'de> {
        unimplemented!()
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value> where
        V: Visitor<'de> {
        unimplemented!()
    }

    fn is_human_readable(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;

    #[test]
    fn bool_false() {
        assert_eq!(from_bytes::<bool>(&[0x19]).unwrap(), false);
    }

    #[test]
    fn bool_true() {
        assert_eq!(from_bytes::<bool>(&[0x1a]).unwrap(), true);
    }

    #[test]
    fn unit() {
        assert_eq!(from_bytes::<()>(&[0x18]).unwrap(), ());
    }

    #[test]
    fn f32() {
        assert_eq!(from_bytes::<f32>(&[0x1b, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]).unwrap(), 0.0);
        assert_eq!(from_bytes::<f32>(&[0x1b, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xf0, 0x3f]).unwrap(), 1.0);
        assert_eq!(from_bytes::<f32>(&[0x1b, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xf0, 0xbf]).unwrap(), -1.0);
    }

    #[test]
    fn f64() {
        assert_eq!(from_bytes::<f64>(&[0x1b, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]).unwrap(), 0.0);
        assert_eq!(from_bytes::<f64>(&[0x1b, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xf0, 0x3f]).unwrap(), 1.0);
        assert_eq!(from_bytes::<f64>(&[0x1b, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xf0, 0xbf]).unwrap(), -1.0);
    }

    #[test]
    fn u8() {
        for i in 0..10 {
            assert_eq!(from_bytes::<u8>(&[0x30 + i]).unwrap(), i);
        }

        // uint, little endian, 1 byte
        assert_eq!(from_bytes::<u8>(&[0x28, 0x0a]).unwrap(), 10);
        assert_eq!(from_bytes::<u8>(&[0x28, 0xff]).unwrap(), std::u8::MAX);
    }

    #[test]
    fn u64() {
        for i in 0..10 {
            assert_eq!(from_bytes::<u64>(&[0x30 + i]).unwrap(), i as u64);
        }

        assert_eq!(from_bytes::<u64>(&[0x28, 0x0a]).unwrap(), 10);
        assert_eq!(from_bytes::<u64>(&[0x2f, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]).unwrap(), std::u64::MAX);
    }

    #[test]
    fn i8() {
        // small negative integers
        for i in 1..7 {
            assert_eq!(from_bytes::<i8>(&[0x40 - i]).unwrap(), -(i as i8));
        }

        for i in 0..10 {
            assert_eq!(from_bytes::<i8>(&[0x30 + i]).unwrap(), i as i8);
        }

        // signed int, little endian, 1 byte
        assert_eq!(from_bytes::<i8>(&[0x20, 0x80]).unwrap(), std::i8::MIN);
        assert_eq!(from_bytes::<i8>(&[0x28, 0x7f]).unwrap(), std::i8::MAX);
        assert_eq!(from_bytes::<i8>(&[0x20, 0xf9]).unwrap(), -7_i8);
        assert_eq!(from_bytes::<i8>(&[0x28, 0x0a]).unwrap(), 10_i8);
    }

    #[test]
    fn string() {
        assert_eq!(from_bytes::<String>(&[0x40]).unwrap(), "".to_owned());
        assert_eq!(from_bytes::<String>(&[0x43, 0x66, 0x6f, 0x6f]).unwrap(), "foo".to_owned());
        assert_eq!(from_bytes::<String>(&[0xa7, 0xe2, 0x88, 0x80, 0xe2, 0x88, 0x82, 0xe2, 0x88, 0x88, 0xe2, 0x84, 0x9d, 0xe2, 0x88, 0xa7,
            0xe2, 0x88, 0xaa, 0xe2, 0x89, 0xa1, 0xe2, 0x88, 0x9e, 0x20, 0xe2, 0x86, 0x91, 0xe2, 0x86, 0x97,
            0xe2, 0x86, 0xa8, 0xe2, 0x86, 0xbb, 0xe2, 0x87, 0xa3, 0x20, 0xe2, 0x94, 0x90, 0xe2, 0x94, 0xbc,
            0xe2, 0x95, 0x94, 0xe2, 0x95, 0x98, 0xe2, 0x96, 0x91, 0xe2, 0x96, 0xba, 0xe2, 0x98, 0xba, 0xe2,
            0x99, 0x80, 0x20, 0xef, 0xac, 0x81, 0xef, 0xbf, 0xbd, 0xe2, 0x91, 0x80, 0xe2, 0x82, 0x82, 0xe1,
            0xbc, 0xa0, 0xe1, 0xb8, 0x82, 0xd3, 0xa5, 0xe1, 0xba, 0x84, 0xc9, 0x90, 0xcb, 0x90, 0xe2, 0x8d,
            0x8e, 0xd7, 0x90, 0xd4, 0xb1, 0xe1, 0x83, 0x90]).unwrap(), "∀∂∈ℝ∧∪≡∞ ↑↗↨↻⇣ ┐┼╔╘░►☺♀ ﬁ�⑀₂ἠḂӥẄɐː⍎אԱა".to_owned());
        assert_eq!(from_bytes::<String>(&[0xbf, 0xa5, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61,
            0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61,
            0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61,
            0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61,
            0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61,
            0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61,
            0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61,
            0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61,
            0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61,
            0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61,
            0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61, 0x61]).unwrap(), "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned());
    }
}
