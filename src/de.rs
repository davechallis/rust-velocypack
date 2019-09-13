use serde::Deserialize;
use serde::de::{
    self, DeserializeSeed, EnumAccess, IntoDeserializer, MapAccess, SeqAccess,
    VariantAccess, Visitor,
};
use bitvec::prelude::{BitVec, LittleEndian};
use bitvec::bits::BitsMut;

use crate::error::{Error, Result};
use std::convert::TryFrom;
use crate::{U8_SIZE, U16_SIZE, U32_SIZE, U64_SIZE};
use std::io::BufRead;
use std::slice::SliceIndex;

pub struct Deserializer<'de> {
    input: &'de [u8],
}

impl<'de> Deserializer<'de> {
    pub fn from_bytes(input: &'de [u8]) -> Self {
        Self { input }
    }

    fn peek_byte(&self) -> Result<u8> {
        match self.input.get(0) {
            Some(b) => Ok(*b),
            None => Err(Error::Message("eof".to_owned())),
        }
    }

    fn peek_bytes<I: SliceIndex<[u8]>>(&self, index: I) -> Result<&<I as SliceIndex<[u8]>>::Output> {
        match self.input.get(index) {
            Some(b) => Ok(b),
            None => Err(Error::Message("eof".to_owned())),
        }
    }

    fn consume_padding(&mut self) -> Result<()> {
        while self.peek_byte()? == 0x00 {
            self.consume_bytes(1);
        }
        Ok(())
    }

    fn next_byte(&mut self) -> Result<u8> {
        let b = self.peek_byte()?;
        self.consume_bytes(1);
        Ok(b)
    }

    fn consume_bytes(&mut self, n: usize) {
        self.input = &self.input[n..]
    }

    fn consume_u8(&mut self) -> Result<u8> {
        let mut bytes: [u8; U8_SIZE] = Default::default();
        bytes.copy_from_slice(self.peek_bytes(..U8_SIZE)?);
        self.consume_bytes(U8_SIZE);
        Ok(u8::from_le_bytes(bytes))
    }

    fn consume_u16(&mut self) -> Result<u16> {
        let mut bytes: [u8; U16_SIZE] = Default::default();
        bytes.copy_from_slice(self.peek_bytes(..U16_SIZE)?);
        self.consume_bytes(U16_SIZE);
        Ok(u16::from_le_bytes(bytes))
    }

    fn consume_u32(&mut self) -> Result<u32> {
        let mut bytes: [u8; U32_SIZE] = Default::default();
        bytes.copy_from_slice(self.peek_bytes(..U32_SIZE)?);
        self.consume_bytes(U32_SIZE);
        Ok(u32::from_le_bytes(bytes))
    }

    fn consume_u64(&mut self) -> Result<u64> {
        let mut bytes: [u8; U64_SIZE] = Default::default();
        bytes.copy_from_slice(self.peek_bytes(..U64_SIZE)?);
        self.consume_bytes(U64_SIZE);
        Ok(u64::from_le_bytes(bytes))
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
                self.consume_bytes(1); // header
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
            x if x >= 0x01 && x <= 0x09 => self.deserialize_seq(visitor),
            x if x >= 0x0a && x <= 0x12 => self.deserialize_map(visitor),
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
        visitor.visit_string(self.parse_string()?)
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

    fn deserialize_seq<V>(mut self, visitor: V) -> Result<V::Value> where
        V: Visitor<'de> {
        visitor.visit_seq(ArrayDeserializer::new(&mut self))
    }

    fn deserialize_tuple<V>(self, len: usize, visitor: V) -> Result<V::Value> where
        V: Visitor<'de> {
        unimplemented!()
    }

    fn deserialize_tuple_struct<V>(self, name: &'static str, len: usize, visitor: V) -> Result<V::Value> where
        V: Visitor<'de> {
        unimplemented!()
    }

    fn deserialize_map<V>(mut self, visitor: V) -> Result<V::Value> where
        V: Visitor<'de> {
        visitor.visit_map(MapDeserializer::new(&mut self))
    }

    fn deserialize_struct<V>(mut self, _name: &'static str, _fields: &'static [&'static str], visitor: V) -> Result<V::Value> where
        V: Visitor<'de> {
        visitor.visit_map(MapDeserializer::new(&mut self))
    }

    fn deserialize_enum<V>(self, name: &'static str, variants: &'static [&'static str], visitor: V) -> Result<V::Value> where
        V: Visitor<'de> {
        unimplemented!()
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value> where
        V: Visitor<'de> {
        self.deserialize_string(visitor)
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value> where
        V: Visitor<'de> {
        unimplemented!()
    }

    fn is_human_readable(&self) -> bool {
        false
    }
}

struct MapDeserializer<'a, 'de: 'a> {
    de: &'a mut Deserializer<'de>,
    index_size: Option<usize>,
    remaining_items: Option<usize>,
}

impl<'a, 'de> MapDeserializer<'a, 'de> {
    fn new(de: &'a mut Deserializer<'de>) -> Self {
        Self { de, index_size: None, remaining_items: None }
    }
}

impl<'de, 'a> MapAccess<'de> for MapDeserializer<'a, 'de> {
    type Error = Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>> where
        K: DeserializeSeed<'de> {
        if self.remaining_items.is_none() {
            match self.de.peek_byte()? {
                0x0a => {
                    self.de.consume_bytes(1);
                    return Ok(None);
                },
                0x0b | 0x0f => {
                    self.de.consume_bytes(1); // header
                    let byte_len = self.de.consume_u8()? as usize - 1 - 2*U8_SIZE; // sub header, bytelen, nitems
                    let num_items = self.de.consume_u8()? as usize;
                    self.remaining_items = Some(num_items);
                    self.index_size = Some(U8_SIZE * num_items);
                },
                0x0c | 0x10 => {
                    self.de.consume_bytes(1); // header
                    let byte_len = self.de.consume_u16()? as usize - 1 - 2*U16_SIZE; // sub header, bytelen, nitems
                    let num_items = self.de.consume_u16()? as usize;
                    self.remaining_items = Some(num_items);
                    self.index_size = Some(U16_SIZE * num_items);
                },
                0x0d | 0x11 => {
                    self.de.consume_bytes(1); // header
                    let byte_len = self.de.consume_u32()? as usize - 1 - 2*U32_SIZE; // sub header, bytelen, nitems
                    let num_items = self.de.consume_u32()? as usize;
                    self.remaining_items = Some(num_items);
                    self.index_size = Some(U32_SIZE * num_items);
                },
                0x0e | 0x12 => {
                    // FIXME: num items is at end
                    self.de.consume_bytes(1); // header
                    let byte_len = self.de.consume_u64()? as usize - 1 - 2*U64_SIZE; // sub header, bytelen, nitems
                    let num_items = self.de.consume_u64()? as usize;
                    self.remaining_items = Some(num_items);
                    self.index_size = Some(U64_SIZE * num_items);
                },
                _ => return Err(Error::Message("ExpectedObject".to_owned()))
            }
        }

        let remaining_items = self.remaining_items.unwrap();
        if remaining_items == 0 {
            if let Some(index_size) = self.index_size {
                // index is unused, but consume bytes
                self.de.consume_bytes(index_size as usize);
            }
            return Ok(None);
        }

        let v = seed.deserialize(&mut *self.de).map(Some);
        self.remaining_items = Some(remaining_items - 1);
        v
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value> where
        V: DeserializeSeed<'de> {
        seed.deserialize(&mut *self.de)
    }
}

struct ArrayDeserializer<'a, 'de: 'a> {
    de: &'a mut Deserializer<'de>,
    index_size: Option<usize>,
    remaining_items: Option<usize>,
}

impl<'a, 'de> ArrayDeserializer<'a, 'de> {
    fn new(de: &'a mut Deserializer<'de>) -> Self {
        Self { de, index_size: None, remaining_items: None }
    }
}

impl <'de, 'a> SeqAccess<'de> for ArrayDeserializer<'a, 'de> {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>> where
        T: DeserializeSeed<'de> {
        if self.remaining_items.is_none() {
            match self.de.peek_byte()? {
                0x01 => {
                    self.de.consume_bytes(1); // header
                    self.remaining_items = Some(0);
                },
                0x02 => {
                    self.de.consume_bytes(1); // header
                    let byte_length = self.de.consume_u8()? as usize - 1 - U8_SIZE; // sub header + bytelen
                    self.de.consume_padding()?;

                    // num items is unknown until first item is consumed
                    let old_size = self.de.input.len();
                    let v = seed.deserialize(&mut *self.de).map(Some);
                    let item_size = old_size - self.de.input.len();
                    let n_items = byte_length / item_size;
                    self.remaining_items = Some(n_items - 1);
                    return v;
                },
                0x03 => {
                    self.de.consume_bytes(1); // header
                    let byte_length = self.de.consume_u16()? as usize - 1 - U16_SIZE; // header + bytelen
                    self.de.consume_padding()?;

                    // num items is unknown until first item is consumed
                    let old_size = self.de.input.len();
                    let v = seed.deserialize(&mut *self.de).map(Some);
                    let item_size = old_size - self.de.input.len();
                    let n_items = byte_length / item_size;
                    self.remaining_items = Some(n_items - 1);
                    return v;
                },
                0x04 => {
                    self.de.consume_bytes(1); // header
                    let byte_length = self.de.consume_u32()? as usize - 1 - U32_SIZE; // header + bytelen
                    self.de.consume_padding()?;

                    // num items is unknown until first item is consumed
                    let old_size = self.de.input.len();
                    let v = seed.deserialize(&mut *self.de).map(Some);
                    let item_size = old_size - self.de.input.len();
                    let n_items = byte_length / item_size;
                    self.remaining_items = Some(n_items - 1);
                    return v;
                },
                0x05 => {
                    self.de.consume_bytes(1); // header
                    let byte_length = self.de.consume_u64()? as usize - 1 - U64_SIZE; // header + bytelen
                    self.de.consume_padding()?;

                    // num items is unknown until first item is consumed
                    let old_size = self.de.input.len();
                    let v = seed.deserialize(&mut *self.de).map(Some);
                    let item_size = old_size - self.de.input.len();
                    let n_items = byte_length / item_size;
                    self.remaining_items = Some(n_items - 1);
                    return v;
                },
                0x06 => {
                    self.de.consume_bytes(1 + U8_SIZE); // header + bytelength (unused)

                    let mut bytes: [u8; U8_SIZE] = Default::default();
                    bytes.copy_from_slice(&self.de.input[..U8_SIZE]);
                    let length = u8::from_le_bytes(bytes) as usize;
                    self.de.consume_bytes(U8_SIZE); // num items
                    self.de.consume_padding()?;

                    self.remaining_items = Some(length);
                    self.index_size = Some(length * U8_SIZE);
                },
                0x07 => {
                    self.de.consume_bytes(1 + U16_SIZE); // header + bytelength (unused)

                    let mut bytes: [u8; U16_SIZE] = Default::default();
                    bytes.copy_from_slice(&self.de.input[..U16_SIZE]);
                    let length = u16::from_le_bytes(bytes) as usize;
                    self.de.consume_bytes(U16_SIZE); // num items
                    self.de.consume_padding()?;

                    self.remaining_items = Some(length);
                    self.index_size = Some(length * U16_SIZE);
                },
                0x08 => {
                    self.de.consume_bytes(1 + U32_SIZE); // header + bytelength (unused)

                    let mut bytes: [u8; U32_SIZE] = Default::default();
                    bytes.copy_from_slice(&self.de.input[..U32_SIZE]);
                    let length = u32::from_le_bytes(bytes) as usize;
                    self.de.consume_bytes(U32_SIZE); // num items
                    self.de.consume_padding()?;

                    self.remaining_items = Some(length);
                    self.index_size = Some(length * U32_SIZE);
                },
                0x09 => {
                    self.de.consume_bytes(1 + U64_SIZE); // header + bytelength (unused)

                    let mut bytes: [u8; U64_SIZE] = Default::default();
                    bytes.copy_from_slice(&self.de.input[..U64_SIZE]);
                    let length = u64::from_le_bytes(bytes) as usize;
                    self.de.consume_bytes(U64_SIZE); // num items
                    self.de.consume_padding()?;

                    self.remaining_items = Some(length);
                    self.index_size = Some(length * U64_SIZE);
                },
                0x13 => {
                    self.de.consume_bytes(1);

                    let mut buf: [u8; 8] = [0; 8];
                    let mut length_bits = buf.as_mut_bitslice::<LittleEndian>();

                    let mut header_size = 1; // header, increment with bytelen bytes
                    let mut idx = 0;
                    loop {
                        let b = self.de.next_byte()?;
                        for n in 0..7 {
                            if (b & (1 << n)) != 0 {
                                *length_bits.at(idx) = true;
                            }
                            idx += 1;
                        }

                        header_size += 1;

                        if (b & (1 << 7)) == 0 { // check high bit set
                            break;
                        }
                    }

                    let bytelength = dbg!(u64::from_le_bytes(buf)) as usize;
                    dbg!(header_size);

                    let remaining_bytes = bytelength - header_size;

                    let mut buf: [u8; 8] = [0; 8];
                    let mut length_bits = buf.as_mut_bitslice::<LittleEndian>();
                    let mut index_size = 0;

                    let mut idx = 0;
                    for b in self.de.input[..remaining_bytes].iter().rev() {
                        for n in 0..7 {
                            if (b & (1 << n)) != 0 {
                                *length_bits.at(idx) = true;
                            }
                            idx += 1;
                        }

                        index_size += 1;

                        if (b & (1 << 7)) == 0 { // check high bit set
                            break;
                        }
                    }

                    let num_items = dbg!(u64::from_le_bytes(buf)) as usize;
                    self.remaining_items = Some(num_items);
                    self.index_size = Some(index_size);
                }
                _ => return Err(Error::Message("ExpectedArray".to_owned()))
            }
        }

        let remaining_items = self.remaining_items.unwrap();
        if remaining_items == 0 {
            if let Some(index_size) = self.index_size {
                // index is unused, but consume bytes
                self.de.consume_bytes(index_size as usize);
            }
            return Ok(None);
        }

        let v = seed.deserialize(&mut *self.de).map(Some);
        self.remaining_items = Some(remaining_items - 1);
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;
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

    #[test]
    fn array_empty() {
        assert_eq!(from_bytes::<Vec<u32>>(&[0x01]).unwrap(), Vec::<u32>::new());
    }

    #[test]
    fn array_no_index() {
        assert_eq!(from_bytes::<Vec<u8>>(&[0x02, 0x05, 0x31, 0x32, 0x33]).unwrap(), vec![1, 2, 3]);
        assert_eq!(from_bytes::<Vec<String>>(&[0x02, 0x06, 0x43, 0x66, 0x6f, 0x6f]).unwrap(), vec!["foo".to_owned()]);


        assert_eq!(from_bytes::<Vec<u8>>(&vec![0x03, 0x02, 0x01, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
                                     0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
                                     0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
                                     0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
                                     0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
                                     0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
                                     0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
                                     0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
                                     0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
                                     0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
                                     0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
                                     0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
                                     0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
                                     0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
                                     0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
                                     0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
                                     0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31]).unwrap(),
        vec![1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
             1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
             1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
             1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
             1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
             1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
             1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
             1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
             1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1]);

    }

    #[test]
    fn array_with_index() {
        assert_eq!(from_bytes::<Vec<u16>>(&[0x06, 0x09, 0x02, 0x31, 0x29, 0x00, 0x01, 0x03, 0x04]).unwrap(), vec![1, 256]);

        assert_eq!(from_bytes::<Vec<Vec<String>>>(&[0x06, 0x1e, 0x03, 0x02, 0x06, 0x43, 0x66, 0x6f, 0x6f, 0x02, 0x0a, 0x43, 0x62, 0x61, 0x72, 0x43,
            0x62, 0x61, 0x7a, 0x02, 0x08, 0x41, 0x61, 0x41, 0x62, 0x41, 0x63, 0x03, 0x09, 0x13]).unwrap(),
                   vec![vec!["foo".to_owned()],
                        vec!["bar".to_owned(), "baz".to_owned()],
                        vec!["a".to_owned(), "b".to_owned(), "c".to_owned()]]);


        assert_eq!(
            from_bytes::<Vec<String>>(&vec![0x07, 0x1f, 0x01, 0x09, 0x00, 0x00, 0x00, 0x00, 0x00, 0xbf, 0xdd, 0x00, 0x00, 0x00, 0x00, 0x00,
0x00, 0x00, 0x61, 0x61, 0x61, 0x64, 0x6b, 0x6c, 0x6a, 0x66, 0x68, 0x64, 0x6b, 0x6c, 0x6a, 0x68,
0x66, 0x6b, 0x6c, 0x64, 0x6a, 0x68, 0x66, 0x6c, 0x6b, 0x6a, 0x64, 0x68, 0x73, 0x64, 0x66, 0x6a,
0x6c, 0x73, 0x68, 0x61, 0x6c, 0x6b, 0x66, 0x6a, 0x73, 0x68, 0x64, 0x66, 0x6c, 0x6b, 0x6a, 0x73,
0x64, 0x68, 0x66, 0x6c, 0x6b, 0x6a, 0x64, 0x68, 0x66, 0x6b, 0x61, 0x6c, 0x6a, 0x68, 0x66, 0x6c,
0x6b, 0x61, 0x73, 0x6a, 0x64, 0x68, 0x66, 0x6c, 0x6b, 0x6a, 0x64, 0x73, 0x68, 0x66, 0x6b, 0x6c,
0x6a, 0x73, 0x64, 0x68, 0x66, 0x6c, 0x6b, 0x6a, 0x64, 0x68, 0x6c, 0x66, 0x6b, 0x6a, 0x68, 0x64,
0x6c, 0x6b, 0x66, 0x6a, 0x68, 0x64, 0x73, 0x6c, 0x6b, 0x66, 0x6a, 0x64, 0x68, 0x61, 0x73, 0x6c,
0x66, 0x6b, 0x6a, 0x61, 0x73, 0x68, 0x64, 0x6c, 0x66, 0x6b, 0x6a, 0x64, 0x73, 0x68, 0x61, 0x6c,
0x66, 0x6b, 0x6a, 0x64, 0x73, 0x68, 0x66, 0x6c, 0x6b, 0x64, 0x6a, 0x73, 0x68, 0x66, 0x6c, 0x6b,
0x73, 0x6a, 0x68, 0x66, 0x6c, 0x73, 0x64, 0x6b, 0x6a, 0x66, 0x68, 0x64, 0x73, 0x6b, 0x6c, 0x6a,
0x66, 0x68, 0x73, 0x61, 0x6c, 0x6b, 0x6a, 0x66, 0x68, 0x64, 0x6c, 0x6b, 0x6a, 0x66, 0x68, 0x61,
0x73, 0x64, 0x6b, 0x6c, 0x6a, 0x68, 0x66, 0x6c, 0x6b, 0x64, 0x73, 0x61, 0x6a, 0x68, 0x66, 0x6c,
0x6b, 0x6a, 0x64, 0x73, 0x68, 0x66, 0x6b, 0x6c, 0x64, 0x6a, 0x68, 0x66, 0x6c, 0x6b, 0x6a, 0x64,
0x73, 0x68, 0x66, 0x6c, 0x6b, 0x6a, 0x61, 0x64, 0x68, 0x66, 0x6c, 0x6b, 0x6a, 0x64, 0x68, 0x41,
0x61, 0x41, 0x61, 0x41, 0x61, 0x42, 0x62, 0x62, 0x43, 0x63, 0x63, 0x63, 0x44, 0x64, 0x64, 0x64,
0x64, 0x44, 0x65, 0x65, 0x65, 0x65, 0x46, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x09, 0x00, 0xef,
0x00, 0xf1, 0x00, 0xf3, 0x00, 0xf5, 0x00, 0xf8, 0x00, 0xfc, 0x00, 0x01, 0x01, 0x06, 0x01]).unwrap(),
            vec!["aaadkljfhdkljhfkldjhflkjdhsdfjlshalkfjshdflkjsdhflkjdhfkaljhflkasjdhflkjdshfkljsdhflkjdhlfkjhdlkfjhdslkfjdhaslfkjashdlfkjdshalfkjdshflkdjshflksjhflsdkjfhdskljfhsalkjfhdlkjfhasdkljhflkdsajhflkjdshfkldjhflkjdshflkjadhflkjdh".to_owned(), "a".to_owned(), "a".to_owned(), "a".to_owned(), "bb".to_owned(), "ccc".to_owned(), "dddd".to_owned(), "eeee".to_owned(), "ffffff".to_owned()]);
    }

    #[test]
    fn array_compact() {
        assert_eq!(from_bytes::<Vec<u8>>(&[0x13, 0x06, 0x31, 0x32, 0x33, 0x03]).unwrap(), vec![1, 2, 3]);
        assert_eq!(from_bytes::<Vec<u8>>(&[0x13, 0x06, 0x31, 0x28, 0x10, 0x02]).unwrap(), vec![1, 16]);
        assert_eq!(from_bytes::<Vec<u8>>(&[0x13, 0xef, 0x05, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
            0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
            0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
            0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
            0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
            0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
            0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
            0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
            0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
            0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
            0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
            0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
            0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
            0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
            0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
            0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
            0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
            0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
            0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
            0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
            0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
            0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
            0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
            0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
            0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
            0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
            0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
            0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
            0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
            0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
            0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
            0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
            0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
            0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
            0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
            0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
            0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
            0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
            0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
            0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
            0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
            0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
            0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
            0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
            0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
            0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
            0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x05, 0xea]).unwrap(), vec![1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1]);
    }

    #[test]
    fn array_123_examples() {
        let expected = vec![1, 2, 3];
        assert_eq!(from_bytes::<Vec<u64>>(&[0x02, 0x05, 0x31, 0x32, 0x33]).unwrap(), expected);
        assert_eq!(from_bytes::<Vec<u64>>(&[0x03, 0x06, 0x00, 0x31, 0x32, 0x33]).unwrap(), expected);
        assert_eq!(from_bytes::<Vec<u64>>(&[0x04, 0x08, 0x00, 0x00, 0x00, 0x31, 0x32, 0x33]).unwrap(), expected);
        assert_eq!(from_bytes::<Vec<u64>>(&[0x05, 0x0c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x31, 0x32, 0x33]).unwrap(), expected);
        assert_eq!(from_bytes::<Vec<u64>>(&[0x06, 0x09, 0x03, 0x31, 0x32, 0x33, 0x03, 0x04, 0x05]).unwrap(), expected);
        assert_eq!(from_bytes::<Vec<u64>>(&[0x07, 0x0e, 0x00, 0x03, 0x00, 0x31, 0x32, 0x33, 0x05, 0x00, 0x06, 0x00, 0x07, 0x00]).unwrap(), expected);
        assert_eq!(from_bytes::<Vec<u64>>(&[0x08, 0x18, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x31, 0x32, 0x33, 0x09, 0x00, 0x00, 0x00, 0x0a, 0x00, 0x00, 0x00, 0x0b, 0x00, 0x00, 0x00]).unwrap(), expected);
//        assert_eq!(from_bytes::<Vec<u64>>(&[0x09, 0x2c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x31, 0x32, 0x33, 0x09, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0b, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]).unwrap(), expected);
    }

    #[test]
    fn object_empty() {
        assert_eq!(from_bytes::<HashMap<String, u8>>(&[0x0a]).unwrap(), HashMap::new());
    }

    #[test]
    fn object_1byte() {
        let mut m = HashMap::new();
        m.insert("a".to_owned(), 1);
        m.insert("b".to_owned(), 2);
        assert_eq!(from_bytes::<HashMap<String, u8>>(&[0x0b, 0x0b, 0x02, 0x41, 0x61, 0x31, 0x41, 0x62, 0x32, 0x03, 0x06]).unwrap(), m);
    }

    #[test]
    fn object_to_struct() {
        #[derive(Debug, Deserialize, PartialEq, Eq)]
        struct Person {
            name: String,
            age: u32,
        }

        assert_eq!(from_bytes::<Person>(&[0x0b, 0x14, 0x02, 0x44, 0x6e, 0x61, 0x6d, 0x65, 0x43, 0x42, 0x6f, 0x62, 0x43, 0x61, 0x67, 0x65,
            0x28, 0x17, 0x0c, 0x03]).unwrap(), Person { name: "Bob".to_owned(), age: 23 });
    }
}
