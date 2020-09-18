use std::fmt::{Display};
use serde::{ser, Serialize};

use crate::error::{Error, Result};

#[derive(Default)]
pub struct Serializer {
    // empty byte list, appended to as values are serialized
    output: Vec<u8>,
}

// by convention, public API of a Serde serializer is one or more
// `to_abc` functions, e.g. `to-string`, `to_bytes`, `to_writer` etc.
pub fn to_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let mut serializer = Serializer::default();
    value.serialize(&mut serializer)?;
    Ok(serializer.output)
}

impl Serializer {
    fn serialize_negative_int(&mut self, v: i64) {
        assert!(v < 0);
        match v {
            i if i > -7 => self.output.push((0x40 + i) as u8),
            i => {
                let b = dbg!(i.to_le_bytes());

                for bit in (0..8).rev() {

                    if b[bit] != 0xff {
                        if bit == 0 && b[bit] < 0x80 {
                            self.output.push((0x20 + bit + 1) as u8);
                            self.output.extend_from_slice(&b[..bit + 1]);
                            self.output.push(0xff);
                        } else {
                            self.output.push((0x20 + bit) as u8);
                            self.output.extend_from_slice(&b[..bit + 1]);
                        }
                        break;
                    }
                }
            },
        }
    }

    fn serialize_unsigned_int(&mut self, v: u64) {
        match v {
            i if i < 10 => self.output.push(0x30 + v as u8),
            i => {
                let b = i.to_le_bytes();
                for bit in (0..8).rev() {
                    if b[bit] != 0x00 {
                        self.output.push(0x28 + bit as u8);
                        self.output.extend_from_slice(&b[..bit + 1]);
                        break;
                    }
                }
            },
        }
    }
}

impl<'a> ser::Serializer for &'a mut Serializer {
    type Ok = ();

    type Error = Error;

    type SerializeSeq = ArraySerializer<'a>;
    type SerializeTuple = ArraySerializer<'a>;
    type SerializeTupleStruct = Self;
    type SerializeTupleVariant = Self;
    type SerializeMap = MapSerializer<'a>;
    type SerializeStruct = MapSerializer<'a>;
    type SerializeStructVariant = Self;

    fn serialize_bool(self, v: bool) -> Result<Self::Ok> {
        self.output.push(if v { 0x1a } else { 0x19 });
        Ok(())
    }

    fn serialize_i8(self, v: i8) -> Result<Self::Ok> {
        if v >= 0 {
            self.serialize_unsigned_int(v as u64);
        } else {
            self.serialize_negative_int(v as i64);
        }
        Ok(())
    }

    fn serialize_i16(self, v: i16) -> Result<Self::Ok> {
        if v >= 0 {
            self.serialize_unsigned_int(v as u64);
        } else {
            self.serialize_negative_int(v as i64);
        }
        Ok(())
    }

    fn serialize_i32(self, v: i32) -> Result<Self::Ok> {
        if v >= 0 {
            self.serialize_unsigned_int(v as u64);
        } else {
            self.serialize_negative_int(v as i64);
        }
        Ok(())
    }

    fn serialize_i64(self, v: i64) -> Result<Self::Ok> {
        if v >= 0 {
            self.serialize_unsigned_int(v as u64);
        } else {
            self.serialize_negative_int(v as i64);
        }
        Ok(())
    }

    fn serialize_u8(self, v: u8) -> Result<Self::Ok> {
        self.serialize_unsigned_int(v as u64);
        Ok(())
    }

    fn serialize_u16(self, v: u16) -> Result<Self::Ok> {
        self.serialize_unsigned_int(v as u64);
        Ok(())
    }

    fn serialize_u32(self, v: u32) -> Result<Self::Ok> {
        self.serialize_unsigned_int(v as u64);
        Ok(())
    }

    fn serialize_u64(self, v: u64) -> Result<Self::Ok> {
        self.serialize_unsigned_int(v);
        Ok(())
    }

    fn serialize_f32(self, v: f32) -> Result<Self::Ok> {
        self.serialize_f64(v as f64)
    }

    fn serialize_f64(self, v: f64) -> Result<Self::Ok> {
        self.output.push(0x1b);
        self.output.extend_from_slice(&v.to_bits().to_le_bytes());
        Ok(())
    }

    fn serialize_char(self, v: char) -> Result<Self::Ok> {
        self.serialize_str(&v.to_string())
    }

    fn serialize_str(self, v: &str) -> Result<Self::Ok> {
        if v.is_empty() {
            self.output.push(0x40);
        } else {
            let b = v.as_bytes();
            let length = b.len();
            if length <= 126 {
                self.output.push(0x40 + length as u8);
            } else {
                self.output.push(0xbf);
                self.output.extend_from_slice(&(length as u64).to_le_bytes());
            }
            self.output.extend_from_slice(b);
        }
        Ok(())
    }

    fn serialize_bytes(self, v: &[u8]) -> Result<Self::Ok> {
        let b = v.len().to_le_bytes();
        for bit in (0..7).rev() {
            if b[bit] != 0x00 {
                self.output.push(0xc0 + bit as u8);
                self.output.extend_from_slice(&b[..bit + 1]);
                break;
            }
        }
        Ok(())
    }

    // use null to represent no value
    fn serialize_none(self) -> Result<Self::Ok> {
        self.output.push(0x18);
        Ok(())
    }

    // no way to express this, just use value
    fn serialize_some<T>(self, value: &T) -> Result<Self::Ok> where
        T: ?Sized + Serialize
    {
        value.serialize(self)
    }

    // use null to represent anonymous value containing no data
    fn serialize_unit(self) -> Result<Self::Ok> {
        self.output.push(0x18);
        Ok(())
    }

    // named valyue containing no data, so map to null
    fn serialize_unit_struct(self, _name: &'static str) -> Result<Self::Ok> {
        self.output.push(0x18);
        Ok(())
    }

    // same behaviour as json
    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str
    ) -> Result<Self::Ok> {
        self.serialize_str(variant)
    }

    // serialise as insignificant wrapper around data contained
    fn serialize_newtype_struct<T>(self, _name: &'static str, value: &T) -> Result<Self::Ok> where
        T: ?Sized + Serialize {
        value.serialize(self)
    }

    // serialise as JSON in externally tagged form as `{ NAME: VALUE }`.
    fn serialize_newtype_variant<T>(self, _name: &'static str, _variant_index: u32, _variant: &'static str, _value: &T) -> Result<Self::Ok> where
        T: ?Sized + Serialize {
        unimplemented!()
    }

    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq> {
        let array_ser = ArraySerializer {
            items: Vec::new(),
            output: &mut self.output,
        };
        Ok(array_ser)
    }

    // serialise as array
    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple> {
        self.serialize_seq(Some(len))
    }

    // serialise as array
    fn serialize_tuple_struct(self, _name: &'static str, _len: usize) -> Result<Self::SerializeTupleStruct> {
        unimplemented!()
    }

    fn serialize_tuple_variant(self, _name: &'static str, _variant_index: u32, _variant: &'static str, _len: usize) -> Result<Self::SerializeTupleVariant> {
        unimplemented!()
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap> {
        let map_ser = MapSerializer {
            keys: Vec::new(),
            values: Vec::new(),
            output: &mut self.output,
        };
        Ok(map_ser)
    }

    fn serialize_struct(self, _name: &'static str, len: usize) -> Result<Self::SerializeStruct> {
        self.serialize_map(Some(len))
    }

    fn serialize_struct_variant(self, _name: &'static str, _variant_index: u32, _variant: &'static str, _len: usize) -> Result<Self::SerializeStructVariant> {
        unimplemented!()
    }

    fn collect_str<T: ?Sized>(self, _value: &T) -> Result<Self::Ok> where
        T: Display {
        unimplemented!()
    }
}

// Same thing but for tuple structs.
impl<'a> ser::SerializeTupleStruct for &'a mut Serializer {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, _value: &T) -> Result<()>
        where
            T: ?Sized + Serialize,
    {
        unimplemented!()
    }

    fn end(self) -> Result<()> {
        unimplemented!()
    }
}

// Tuple variants are a little different. Refer back to the
// `serialize_tuple_variant` method above:
//
//    self.output += "{";
//    variant.serialize(&mut *self)?;
//    self.output += ":[";
//
// So the `end` method in this impl is responsible for closing both the `]` and
// the `}`.
impl<'a> ser::SerializeTupleVariant for &'a mut Serializer {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, _value: &T) -> Result<()>
        where
            T: ?Sized + Serialize,
    {
        unimplemented!()
    }

    fn end(self) -> Result<()> {
        unimplemented!()
    }
}

// Structs are like maps in which the keys are constrained to be compile-time
// constant strings.
impl<'a> ser::SerializeStruct for &'a mut Serializer {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, _key: &'static str, _value: &T) -> Result<()>
        where
            T: ?Sized + Serialize,
    {
        unimplemented!()
    }

    fn end(self) -> Result<()> {
        unimplemented!()
    }
}

// Similar to `SerializeTupleVariant`, here the `end` method is responsible for
// closing both of the curly braces opened by `serialize_struct_variant`.
impl<'a> ser::SerializeStructVariant for &'a mut Serializer {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, _key: &'static str, _value: &T) -> Result<()>
        where
            T: ?Sized + Serialize,
    {
        unimplemented!()
    }

    fn end(self) -> Result<()> {
        unimplemented!()
    }
}

pub struct MapSerializer<'a> {
    keys: Vec<Vec<u8>>,
    values: Vec<Vec<u8>>,
    output: &'a mut Vec<u8>,
}

impl <'a> MapSerializer<'a> {
    fn serialize_map_key<T>(&mut self, key: &T) -> Result<()> where
        T: ?Sized + Serialize {
        let mut serializer = Serializer::default();
        key.serialize(&mut serializer)?;
        let header = match serializer.output.first() {
            Some(header) => header,
            None => return Err(Error::Message("Empty serialization".to_owned())),
        };

        if *header >= 0x40_u8 && *header <= 0xbf_u8 {
            self.keys.push(serializer.output);
            Ok(())
        } else {
            Err(Error::Message(format!("Cannot serialize type to map key: {}", header)))
        }
    }

    fn serialize_map_value<T: ?Sized>(&mut self, value: &T) -> Result<()> where
        T: Serialize {
        let mut serializer = Serializer::default();
        value.serialize(&mut serializer)?;
        self.values.push(serializer.output);
        Ok(())
    }

    fn end_map(mut self) -> Result<()> {
        if self.keys.is_empty() {
            assert!(self.values.is_empty());
            self.output.push(0x0a);
            return Ok(());
        }

        assert_eq!(self.keys.len(), self.values.len());

        // 1 byte header
        // 1/2/4/8 bytes total bytelength
        // 1/2/4/8 bytes number of items
        // key/value pairs
        // 1/2/4/8 byte offsets indexing into total data structure
        let mut item_size = 0;
        for key in &self.keys {
            item_size += key.len();
        }
        for value in &self.values {
            item_size += value.len();
        }

        let n_items = self.keys.len();

        // try with 1 byte, then 2, then 4, then 8
        for n_bytes in &[1, 2, 4, 8] {
            // header, bytesize, nritems, <items>, <indexes>
            let needed_size: usize = 1 + n_bytes + n_bytes + item_size + n_items * n_bytes;

            if needed_size < 2_usize.pow((n_bytes * 8) as u32) {
                // add header
                match n_bytes {
                    1 => {
                        self.output.push(0x0b);
                        self.output.extend_from_slice(&(needed_size as u8).to_le_bytes()); // byte size
                        self.output.extend_from_slice(&(n_items as u8).to_le_bytes()); // num items
                    },
                    2 => {
                        self.output.push(0x0c);
                        self.output.extend_from_slice(&(needed_size as u16).to_le_bytes()); // byte size
                        self.output.extend_from_slice(&(n_items as u16).to_le_bytes()); // num items
                    },
                    4 => {
                        self.output.push(0x0d);
                        self.output.extend_from_slice(&(needed_size as u32).to_le_bytes()); // byte size
                        self.output.extend_from_slice(&(n_items as u32).to_le_bytes()); // num items
                    },
                    8 => {
                        self.output.push(0x0e);
                        self.output.extend_from_slice(&(needed_size as u64).to_le_bytes()); // byte size
                        self.output.extend_from_slice(&(n_items as u64).to_le_bytes()); // num items
                    },
                    _ => panic!("Unexpected byte size"),
                }

                let sorted_offset_idx: Vec<usize> = {
                    // build vec of keys and index, then sort them, use for indexing into values
                    let mut sorted_keys: Vec<(usize, &Vec<u8>)> = self.keys
                        .iter()
                        .enumerate()
                        .collect();
                    sorted_keys.sort_by_key(|(_i, v)| v.clone());

                    sorted_keys.iter()
                        .map(|(i, _v)| *i)
                        .collect()
                };

                let mut offsets = Vec::with_capacity(n_items);

                // header, byte size, nritems
                let mut offset = 1 + 2 * n_bytes;

                // write items in given order
                for i in 0..n_items {
                    offsets.push(offset);
                    let mut key = self.keys.get_mut(i).unwrap();
                    let mut value = self.values.get_mut(i).unwrap();
                    offset += key.len() + value.len();
                    self.output.append(&mut key);
                    self.output.append(&mut value);
                }
                assert_eq!(offsets.len(), sorted_offset_idx.len());

                // write offsets index in sorted order
                match n_bytes {
                    1 => {
                        for idx in sorted_offset_idx {
                            self.output.extend_from_slice(&(offsets[idx] as u8).to_le_bytes()); // num items
                        }
                    },
                    2 => {
                        for idx in sorted_offset_idx {
                            self.output.extend_from_slice(&(offsets[idx] as u16).to_le_bytes()); // num items
                        }
                    },
                    4 => {
                        for idx in sorted_offset_idx {
                            self.output.extend_from_slice(&(offsets[idx] as u32).to_le_bytes()); // num items
                        }
                    },
                    8 => {
                        for idx in sorted_offset_idx {
                            self.output.extend_from_slice(&(offsets[idx] as u64).to_le_bytes()); // num items
                        }
                    },
                    _ => panic!("Unexpected byte length"),
                }

                break;
            }
        }
        Ok(())
    }
}

impl <'a> ser::SerializeStruct for MapSerializer<'a> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: ?Sized>(&mut self, key: &'static str, value: &T) -> Result<Self::Ok> where
        T: Serialize {
        self.serialize_map_key(key)?;
        self.serialize_map_value(value)?;
        Ok(())
    }

    fn end(self) -> Result<Self::Ok> {
        self.end_map()
    }
}

impl <'a> ser::SerializeMap for MapSerializer<'a> {
    type Ok = ();
    type Error = Error;

    fn serialize_key<T>(&mut self, key: &T) -> Result<Self::Ok> where
        T: ?Sized + Serialize {
        self.serialize_map_key(key)
    }

    fn serialize_value<T: ?Sized>(&mut self, value: &T) -> Result<Self::Ok> where
        T: Serialize {
        self.serialize_map_value(value)
    }

    fn end(self) -> Result<Self::Ok> {
        self.end_map()
    }
}


pub struct ArraySerializer<'a> {
    items: Vec<Vec<u8>>,
    output: &'a mut Vec<u8>,
}

impl<'a> ArraySerializer<'a> {
    fn serialize_array_element<T>(&mut self, value: &T) -> Result<()> where
        T: ?Sized + Serialize {
        let mut serializer = Serializer::default();
        value.serialize(&mut serializer)?;
        self.items.push(serializer.output);
        Ok(())
    }

    fn end_array(mut self) -> Result<()> {
        if self.items.is_empty() {
            self.output.push(0x01);
        } else {
            let elem_len = self.items[0].len();
            let same_length = self.items
                .iter()
                .all(|ref v| v.len() == elem_len);
            if same_length {
                let byte_size = self.items.len() * elem_len;
                if byte_size < 2_usize.pow(8) - 2 {
                    self.output.push(0x02);
                    self.output.extend_from_slice(&((byte_size + 2) as u8).to_le_bytes());
                } else if byte_size < 2_usize.pow(16) - 3 {
                    self.output.push(0x03);
                    self.output.extend_from_slice(&((byte_size + 3) as u16).to_le_bytes());
                } else if byte_size < 2_usize.pow(32) - 4 {
                    self.output.push(0x04);
                    self.output.extend_from_slice(&((byte_size + 4) as u32).to_le_bytes());
                } else {
                    self.output.push(0x05);
                    self.output.extend_from_slice(&((byte_size + 5) as u64).to_le_bytes());
                };

                for item in &mut self.items.iter_mut() {
                    self.output.append(item);
                }
            } else {
                let n_items = self.items.len();

                // 1 byte header
                // 1/2/4/8 bytes total bytelength
                // 1/2/4/8 bytes number of items
                // data items
                // 1/2/4/8 byte offsets indexing into total data structure
                let mut item_size = 0;
                for item in &self.items {
                    item_size += item.len();
                }

                // try with 1 byte, then 2, then 4, then 8
                for n_bytes in &[1, 2, 4, 8] {
                    // header, bytesize, nritems, <items>, <indexes>
                    let needed_size: usize = 1 + n_bytes + n_bytes + item_size + n_items * n_bytes;

                    if needed_size < 2_usize.pow((n_bytes * 8) as u32) {
                        // add header
                        match n_bytes {
                            1 => {
                                self.output.push(0x06);
                                self.output.extend_from_slice(&(needed_size as u8).to_le_bytes()); // byte size
                                self.output.extend_from_slice(&(n_items as u8).to_le_bytes()); // num items
                            },
                            2 => {
                                self.output.push(0x07);
                                self.output.extend_from_slice(&(needed_size as u16).to_le_bytes()); // byte size
                                self.output.extend_from_slice(&(n_items as u16).to_le_bytes()); // num items
                            },
                            4 => {
                                self.output.push(0x08);
                                self.output.extend_from_slice(&(needed_size as u32).to_le_bytes()); // byte size
                                self.output.extend_from_slice(&(n_items as u32).to_le_bytes()); // num items
                            },
                            8 => {
                                self.output.push(0x09);
                                self.output.extend_from_slice(&(needed_size as u64).to_le_bytes()); // byte size
                                self.output.extend_from_slice(&(n_items as u64).to_le_bytes()); // num items
                            },
                            _ => panic!("Unexpected byte size"),
                        }

                        let mut offsets = Vec::with_capacity(n_items);
                        let mut offset = 1 + 2 * n_bytes;

                        for item in &mut self.items.iter_mut() {
                            offsets.push(offset);
                            offset += item.len();
                            self.output.append(item);
                        }

                        match n_bytes {
                            1 =>  {
                                for offset in offsets {
                                    self.output.extend_from_slice(&(offset as u8).to_le_bytes()); // num items
                                }
                            },
                            2 => {
                                for offset in offsets {
                                    self.output.extend_from_slice(&(offset as u16).to_le_bytes()); // num items
                                }
                            },
                            4 => {
                                for offset in offsets {
                                    self.output.extend_from_slice(&(offset as u32).to_le_bytes()); // num items
                                }
                            },
                            8 => {
                                for offset in offsets {
                                    self.output.extend_from_slice(&(offset as u64).to_le_bytes()); // num items
                                }
                            },
                            _ => panic!("Unexpected byte length"),
                        }

                        break;
                    }
                }
            }
        }
        Ok(())
    }
}

impl <'a> ser::SerializeSeq for ArraySerializer<'a> {
    type Ok = ();
    type Error = Error;

    fn serialize_element<T>(&mut self, value: &T) -> Result<Self::Ok> where
        T: ?Sized + Serialize {
        self.serialize_array_element(value)
    }

    fn end(self) -> Result<Self::Ok> {
        self.end_array()
    }
}


impl <'a> ser::SerializeTuple for ArraySerializer<'a> {
    type Ok = ();
    type Error = Error;

    fn serialize_element<T>(&mut self, value: &T) -> Result<Self::Ok> where
        T: ?Sized + Serialize {
        self.serialize_array_element(value)
    }

    fn end(self) -> Result<Self::Ok> {
        self.end_array()
    }
}

////////////////////////////////////////////////////////////////////////////////
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;

    const U24_MAX: i32 = 16777215;
    const U40_MAX: u64 = 1099511627775;
    const U48_MAX: u64 = 281474976710655;
    const U56_MAX: u64 = 72057594037927935;

    const I24_MAX: i32 = 8388607;
    const I24_MIN: i32 = -8388608;
    const I40_MAX: i64 = 549755813887;
    const I40_MIN: i64 = -549755813888;
    const I48_MAX: i64 = 140737488355327;
    const I48_MIN: i64 = -140737488355328;
    const I56_MAX: i64 = 36028797018963967;
    const I56_MIN: i64 = -36028797018963968;

    #[test]
    fn bool_false() {
        assert_eq!(to_bytes(&false).unwrap(), &[0x19]);
    }

    #[test]
    fn bool_true() {
        assert_eq!(to_bytes(&true).unwrap(), &[0x1a]);
    }

    #[test]
    fn i8() {
        // small negative integers
        assert_eq!(to_bytes(&-6i8).unwrap(), &[0x3a]);
        assert_eq!(to_bytes(&-5i8).unwrap(), &[0x3b]);
        assert_eq!(to_bytes(&-4i8).unwrap(), &[0x3c]);
        assert_eq!(to_bytes(&-3i8).unwrap(), &[0x3d]);
        assert_eq!(to_bytes(&-2i8).unwrap(), &[0x3e]);
        assert_eq!(to_bytes(&-1i8).unwrap(), &[0x3f]);

        // small integers
        assert_eq!(to_bytes(&0i8).unwrap(), &[0x30]);
        assert_eq!(to_bytes(&1i8).unwrap(), &[0x31]);
        assert_eq!(to_bytes(&2i8).unwrap(), &[0x32]);
        assert_eq!(to_bytes(&3i8).unwrap(), &[0x33]);
        assert_eq!(to_bytes(&4i8).unwrap(), &[0x34]);
        assert_eq!(to_bytes(&5i8).unwrap(), &[0x35]);
        assert_eq!(to_bytes(&6i8).unwrap(), &[0x36]);
        assert_eq!(to_bytes(&7i8).unwrap(), &[0x37]);
        assert_eq!(to_bytes(&8i8).unwrap(), &[0x38]);
        assert_eq!(to_bytes(&9i8).unwrap(), &[0x39]);

        // signed int, little endian, 1 byte
        assert_eq!(to_bytes(&std::i8::MIN).unwrap(), &[0x20, 0x80]);
        assert_eq!(to_bytes(&std::i8::MAX).unwrap(), &[0x28, 0x7f]);
        assert_eq!(to_bytes(&-7i8).unwrap(), &[0x20, 0xf9]);
        assert_eq!(to_bytes(&10i8).unwrap(), &[0x28, 0x0a]);
    }

    #[test]
    fn i16() {
        // small negative integers
        assert_eq!(to_bytes(&-6i16).unwrap(), &[0x3a]);
        assert_eq!(to_bytes(&-5i16).unwrap(), &[0x3b]);
        assert_eq!(to_bytes(&-4i16).unwrap(), &[0x3c]);
        assert_eq!(to_bytes(&-3i16).unwrap(), &[0x3d]);
        assert_eq!(to_bytes(&-2i16).unwrap(), &[0x3e]);
        assert_eq!(to_bytes(&-1i16).unwrap(), &[0x3f]);

        // small integers
        assert_eq!(to_bytes(&0i16).unwrap(), &[0x30]);
        assert_eq!(to_bytes(&1i16).unwrap(), &[0x31]);
        assert_eq!(to_bytes(&2i16).unwrap(), &[0x32]);
        assert_eq!(to_bytes(&3i16).unwrap(), &[0x33]);
        assert_eq!(to_bytes(&4i16).unwrap(), &[0x34]);
        assert_eq!(to_bytes(&5i16).unwrap(), &[0x35]);
        assert_eq!(to_bytes(&6i16).unwrap(), &[0x36]);
        assert_eq!(to_bytes(&7i16).unwrap(), &[0x37]);
        assert_eq!(to_bytes(&8i16).unwrap(), &[0x38]);
        assert_eq!(to_bytes(&9i16).unwrap(), &[0x39]);

        // signed int, little endian, 1 byte
        assert_eq!(to_bytes(&(std::i8::MIN as i16)).unwrap(), &[0x20, 0x80]);
        assert_eq!(to_bytes(&(std::i8::MAX as i16)).unwrap(), &[0x28, 0x7f]);
        assert_eq!(to_bytes(&-7i16).unwrap(), &[0x20, 0xf9]);
        assert_eq!(to_bytes(&10i16).unwrap(), &[0x28, 0x0a]);

        // signed int, little endian, 2 bytes
        assert_eq!(to_bytes(&std::i16::MIN).unwrap(), &[0x21, 0x00, 0x80]);
        assert_eq!(to_bytes(&std::i16::MAX).unwrap(), &[0x29, 0xff, 0x7f]);
        assert_eq!(to_bytes(&-12345i16).unwrap(), &[0x21, 0xc7, 0xcf]);
        assert_eq!(to_bytes(&12345i16).unwrap(), &[0x29, 0x39, 0x30]);
    }

    #[test]
    fn test_i32() {
        // small negative integers
        assert_eq!(to_bytes(&-6i32).unwrap(), &[0x3a]);
        assert_eq!(to_bytes(&-5i32).unwrap(), &[0x3b]);
        assert_eq!(to_bytes(&-4i32).unwrap(), &[0x3c]);
        assert_eq!(to_bytes(&-3i32).unwrap(), &[0x3d]);
        assert_eq!(to_bytes(&-2i32).unwrap(), &[0x3e]);
        assert_eq!(to_bytes(&-1i32).unwrap(), &[0x3f]);

        // small integers
        assert_eq!(to_bytes(&0i32).unwrap(), &[0x30]);
        assert_eq!(to_bytes(&1i32).unwrap(), &[0x31]);
        assert_eq!(to_bytes(&2i32).unwrap(), &[0x32]);
        assert_eq!(to_bytes(&3i32).unwrap(), &[0x33]);
        assert_eq!(to_bytes(&4i32).unwrap(), &[0x34]);
        assert_eq!(to_bytes(&5i32).unwrap(), &[0x35]);
        assert_eq!(to_bytes(&6i32).unwrap(), &[0x36]);
        assert_eq!(to_bytes(&7i32).unwrap(), &[0x37]);
        assert_eq!(to_bytes(&8i32).unwrap(), &[0x38]);
        assert_eq!(to_bytes(&9i32).unwrap(), &[0x39]);

        // signed int, little endian, 1 byte
        assert_eq!(to_bytes(&(std::i8::MIN as i32)).unwrap(), &[0x20, 0x80]);
        assert_eq!(to_bytes(&(std::i8::MAX as i32)).unwrap(), &[0x28, 0x7f]);
        assert_eq!(to_bytes(&-7i32).unwrap(), &[0x20, 0xf9]);
        assert_eq!(to_bytes(&10i32).unwrap(), &[0x28, 0x0a]);

        // signed int, little endian, 2 bytes
        assert_eq!(to_bytes(&std::i16::MIN).unwrap(), &[0x21, 0x00, 0x80]);
        assert_eq!(to_bytes(&std::i16::MAX).unwrap(), &[0x29, 0xff, 0x7f]);
        assert_eq!(to_bytes(&-12345i32).unwrap(), &[0x21, 0xc7, 0xcf]);
        assert_eq!(to_bytes(&12345i32).unwrap(), &[0x29, 0x39, 0x30]);

        // signed int, little endian, 3 bytes
        assert_eq!(to_bytes(&I24_MAX).unwrap(), &[0x2a, 0xff, 0xff, 0x7f]);
        assert_eq!(to_bytes(&I24_MIN).unwrap(), &[0x22, 0x00, 0x00, 0x80]);

        // signed int, little endian, 4 bytes
        assert_eq!(to_bytes(&std::i32::MIN).unwrap(), &[0x23, 0x00, 0x00, 0x00, 0x80]);
        assert_eq!(to_bytes(&std::i32::MAX).unwrap(), &[0x2b, 0xff, 0xff, 0xff, 0x7f]);
    }

    #[test]
    fn test_i64() {
        // small negative integers
        assert_eq!(to_bytes(&-6i64).unwrap(), &[0x3a]);
        assert_eq!(to_bytes(&-5i64).unwrap(), &[0x3b]);
        assert_eq!(to_bytes(&-4i64).unwrap(), &[0x3c]);
        assert_eq!(to_bytes(&-3i64).unwrap(), &[0x3d]);
        assert_eq!(to_bytes(&-2i64).unwrap(), &[0x3e]);
        assert_eq!(to_bytes(&-1i64).unwrap(), &[0x3f]);

        // small integers
        assert_eq!(to_bytes(&0i64).unwrap(), &[0x30]);
        assert_eq!(to_bytes(&1i64).unwrap(), &[0x31]);
        assert_eq!(to_bytes(&2i64).unwrap(), &[0x32]);
        assert_eq!(to_bytes(&3i64).unwrap(), &[0x33]);
        assert_eq!(to_bytes(&4i64).unwrap(), &[0x34]);
        assert_eq!(to_bytes(&5i64).unwrap(), &[0x35]);
        assert_eq!(to_bytes(&6i64).unwrap(), &[0x36]);
        assert_eq!(to_bytes(&7i64).unwrap(), &[0x37]);
        assert_eq!(to_bytes(&8i64).unwrap(), &[0x38]);
        assert_eq!(to_bytes(&9i64).unwrap(), &[0x39]);

        // signed int, little endian, 1 byte
        assert_eq!(to_bytes(&(std::i8::MIN as i64)).unwrap(), &[0x20, 0x80]);
        assert_eq!(to_bytes(&(std::i8::MAX as i64)).unwrap(), &[0x28, 0x7f]);
        assert_eq!(to_bytes(&-7i64).unwrap(), &[0x20, 0xf9]);
        assert_eq!(to_bytes(&10i64).unwrap(), &[0x28, 0x0a]);

        // signed int, little endian, 2 bytes
        assert_eq!(to_bytes(&std::i16::MIN).unwrap(), &[0x21, 0x00, 0x80]);
        assert_eq!(to_bytes(&std::i16::MAX).unwrap(), &[0x29, 0xff, 0x7f]);
        assert_eq!(to_bytes(&-12345i64).unwrap(), &[0x21, 0xc7, 0xcf]);
        assert_eq!(to_bytes(&12345i64).unwrap(), &[0x29, 0x39, 0x30]);

        // signed int, little endian, 3 bytes
        assert_eq!(to_bytes(&I24_MIN).unwrap(), &[0x22, 0x00, 0x00, 0x80]);
        assert_eq!(to_bytes(&I24_MAX).unwrap(), &[0x2a, 0xff, 0xff, 0x7f]);

        // signed int, little endian, 4 bytes
        assert_eq!(to_bytes(&std::i32::MIN).unwrap(), &[0x23, 0x00, 0x00, 0x00, 0x80]);
        assert_eq!(to_bytes(&std::i32::MAX).unwrap(), &[0x2b, 0xff, 0xff, 0xff, 0x7f]);

        // signed int, little endian, 5 bytes
        assert_eq!(to_bytes(&I40_MIN).unwrap(), &[0x24, 0x00, 0x00, 0x00, 0x00, 0x80]);
        assert_eq!(to_bytes(&I40_MAX).unwrap(), &[0x2c, 0xff, 0xff, 0xff, 0xff, 0x7f]);

        // signed int, little endian, 6 bytes
        assert_eq!(to_bytes(&I48_MIN).unwrap(), &[0x25, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80]);
        assert_eq!(to_bytes(&I48_MAX).unwrap(), &[0x2d, 0xff, 0xff, 0xff, 0xff, 0xff, 0x7f]);

        // signed int, little endian, 7 bytes
        assert_eq!(to_bytes(&I56_MIN).unwrap(), &[0x26, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80]);
        assert_eq!(to_bytes(&I56_MAX).unwrap(), &[0x2e, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x7f]);

        // signed int, little endian, 8 bytes
        assert_eq!(to_bytes(&std::i64::MIN).unwrap(), &[0x27, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80]);
        assert_eq!(to_bytes(&std::i64::MAX).unwrap(), &[0x2f, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x7f]);
    }

    #[test]
    fn u8() {
        // small integers
        assert_eq!(to_bytes(&0u8).unwrap(), &[0x30]);
        assert_eq!(to_bytes(&1u8).unwrap(), &[0x31]);
        assert_eq!(to_bytes(&2u8).unwrap(), &[0x32]);
        assert_eq!(to_bytes(&3u8).unwrap(), &[0x33]);
        assert_eq!(to_bytes(&4u8).unwrap(), &[0x34]);
        assert_eq!(to_bytes(&5u8).unwrap(), &[0x35]);
        assert_eq!(to_bytes(&6u8).unwrap(), &[0x36]);
        assert_eq!(to_bytes(&7u8).unwrap(), &[0x37]);
        assert_eq!(to_bytes(&8u8).unwrap(), &[0x38]);
        assert_eq!(to_bytes(&9u8).unwrap(), &[0x39]);

        // uint, little endian, 1 byte
        assert_eq!(to_bytes(&10u8).unwrap(), &[0x28, 0x0a]);
        assert_eq!(to_bytes(&std::u8::MAX).unwrap(), &[0x28, 0xff]);
    }

    #[test]
    fn test_u16() {
        // small integers
        assert_eq!(to_bytes(&0u16).unwrap(), &[0x30]);
        assert_eq!(to_bytes(&1u16).unwrap(), &[0x31]);
        assert_eq!(to_bytes(&2u16).unwrap(), &[0x32]);
        assert_eq!(to_bytes(&3u16).unwrap(), &[0x33]);
        assert_eq!(to_bytes(&4u16).unwrap(), &[0x34]);
        assert_eq!(to_bytes(&5u16).unwrap(), &[0x35]);
        assert_eq!(to_bytes(&6u16).unwrap(), &[0x36]);
        assert_eq!(to_bytes(&7u16).unwrap(), &[0x37]);
        assert_eq!(to_bytes(&8u16).unwrap(), &[0x38]);
        assert_eq!(to_bytes(&9u16).unwrap(), &[0x39]);

        // uint, little endian, 1 byte
        assert_eq!(to_bytes(&(std::u8::MAX as u16)).unwrap(), &[0x28, 0xff]);
        assert_eq!(to_bytes(&10u16).unwrap(), &[0x28, 0x0a]);

        // uint, little endian, 2 bytes
        assert_eq!(to_bytes(&std::u16::MAX).unwrap(), &[0x29, 0xff, 0xff]);
        assert_eq!(to_bytes(&12345u16).unwrap(), &[0x29, 0x39, 0x30]);
    }

    #[test]
    fn test_u32() {
        // small integers
        assert_eq!(to_bytes(&0u32).unwrap(), &[0x30]);
        assert_eq!(to_bytes(&1u32).unwrap(), &[0x31]);
        assert_eq!(to_bytes(&2u32).unwrap(), &[0x32]);
        assert_eq!(to_bytes(&3u32).unwrap(), &[0x33]);
        assert_eq!(to_bytes(&4u32).unwrap(), &[0x34]);
        assert_eq!(to_bytes(&5u32).unwrap(), &[0x35]);
        assert_eq!(to_bytes(&6u32).unwrap(), &[0x36]);
        assert_eq!(to_bytes(&7u32).unwrap(), &[0x37]);
        assert_eq!(to_bytes(&8u32).unwrap(), &[0x38]);
        assert_eq!(to_bytes(&9u32).unwrap(), &[0x39]);

        // uint, little endian, 1 byte
        assert_eq!(to_bytes(&(std::u8::MAX as u32)).unwrap(), &[0x28, 0xff]);
        assert_eq!(to_bytes(&10u32).unwrap(), &[0x28, 0x0a]);

        // uint, little endian, 2 bytes
        assert_eq!(to_bytes(&std::u16::MAX).unwrap(), &[0x29, 0xff, 0xff]);
        assert_eq!(to_bytes(&12345u32).unwrap(), &[0x29, 0x39, 0x30]);

        // uint, little endian, 3 bytes
        assert_eq!(to_bytes(&I24_MAX).unwrap(), &[0x2a, 0xff, 0xff, 0x7f]);

        // uint, little endian, 4 bytes
        assert_eq!(to_bytes(&std::u32::MAX).unwrap(), &[0x2b, 0xff, 0xff, 0xff, 0xff]);
    }

    #[test]
    fn test_u64() {
        // small integers
        assert_eq!(to_bytes(&0u64).unwrap(), &[0x30]);
        assert_eq!(to_bytes(&1u64).unwrap(), &[0x31]);
        assert_eq!(to_bytes(&2u64).unwrap(), &[0x32]);
        assert_eq!(to_bytes(&3u64).unwrap(), &[0x33]);
        assert_eq!(to_bytes(&4u64).unwrap(), &[0x34]);
        assert_eq!(to_bytes(&5u64).unwrap(), &[0x35]);
        assert_eq!(to_bytes(&6u64).unwrap(), &[0x36]);
        assert_eq!(to_bytes(&7u64).unwrap(), &[0x37]);
        assert_eq!(to_bytes(&8u64).unwrap(), &[0x38]);
        assert_eq!(to_bytes(&9u64).unwrap(), &[0x39]);

        // uint, little endian, 1 byte
        assert_eq!(to_bytes(&(std::u8::MAX as u64)).unwrap(), &[0x28, 0xff]);
        assert_eq!(to_bytes(&10u64).unwrap(), &[0x28, 0x0a]);

        // uint, little endian, 2 bytes
        assert_eq!(to_bytes(&std::u16::MAX).unwrap(), &[0x29, 0xff, 0xff]);
        assert_eq!(to_bytes(&12345u64).unwrap(), &[0x29, 0x39, 0x30]);

        // uint, little endian, 3 bytes
        assert_eq!(to_bytes(&U24_MAX).unwrap(), &[0x2a, 0xff, 0xff, 0xff]);

        // uint, little endian, 4 bytes
        assert_eq!(to_bytes(&std::u32::MAX).unwrap(), &[0x2b, 0xff, 0xff, 0xff, 0xff]);

        // uint, little endian, 5 bytes
        assert_eq!(to_bytes(&U40_MAX).unwrap(), &[0x2c, 0xff, 0xff, 0xff, 0xff, 0xff]);

        // uint, little endian, 6 bytes
        assert_eq!(to_bytes(&U48_MAX).unwrap(), &[0x2d, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]);

        // uint, little endian, 7 bytes
        assert_eq!(to_bytes(&U56_MAX).unwrap(), &[0x2e, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]);

        // uint, little endian, 8 bytes
        assert_eq!(to_bytes(&std::u64::MAX).unwrap(), &[0x2f, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]);
    }

    #[test]
    fn f32() {
        assert_eq!(to_bytes(&0.0f32).unwrap(), &[0x1b, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
        assert_eq!(to_bytes(&1.0f32).unwrap(), &[0x1b, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xf0, 0x3f]);
        assert_eq!(to_bytes(&-1.0f32).unwrap(), &[0x1b, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xf0, 0xbf]);
    }

    #[test]
    fn f64() {
        assert_eq!(to_bytes(&0.0f64).unwrap(), &[0x1b, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
        assert_eq!(to_bytes(&1.0f64).unwrap(), &[0x1b, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xf0, 0x3f]);
        assert_eq!(to_bytes(&-1.0f64).unwrap(), &[0x1b, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xf0, 0xbf]);
    }

    #[test]
    fn char() {
        assert_eq!(to_bytes(&'a').unwrap(), &[0x41, 0x61]);
        assert_eq!(to_bytes(&'?').unwrap(), &[0x41, 0x3f]);
    }

    #[test]
    fn string() {
        assert_eq!(to_bytes(&"").unwrap(),  &[0x40]);
        assert_eq!(to_bytes(&"a").unwrap(),  &[0x41, 0x61]);
        assert_eq!(to_bytes(&"?").unwrap(),  &[0x41, 0x3f]);
        assert_eq!(to_bytes(&"The quick brown fox jumps over the lazy dog.").unwrap(), vec![
            0x6c, 0x54, 0x68, 0x65, 0x20, 0x71, 0x75, 0x69, 0x63, 0x6b, 0x20, 0x62, 0x72, 0x6f, 0x77, 0x6e,
            0x20, 0x66, 0x6f, 0x78, 0x20, 0x6a, 0x75, 0x6d, 0x70, 0x73, 0x20, 0x6f, 0x76, 0x65, 0x72, 0x20,
            0x74, 0x68, 0x65, 0x20, 0x6c, 0x61, 0x7a, 0x79, 0x20, 0x64, 0x6f, 0x67, 0x2e
        ]);
        assert_eq!(to_bytes(&"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA").unwrap(),
                   vec![0xbf, 0x97, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41,
                       0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41,
                       0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41,
                       0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41,
                       0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41,
                       0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41,
                       0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41,
                       0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41,
                       0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41,
                       0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41]);
    }

    #[test]
    fn test_bytes() {
        // TODO
    }

    #[test]
    fn none() {
        let o: Option<u32> = None;
        assert_eq!(to_bytes(&o).unwrap(),  &[0x18]);
    }

    #[test]
    fn some() {
        let o: Option<bool> = Some(true);
        assert_eq!(to_bytes(&o).unwrap(),  &[0x1a]);
    }

    #[test]
    fn unit() {
        assert_eq!(to_bytes(&()).unwrap(),  &[0x18]);
    }

    #[test]
    fn unit_struct() {
        assert_eq!(to_bytes(&()).unwrap(),  &[0x18]);
    }

    #[test]
    fn unit_variant() {
        // TODO
    }

    #[test]
    fn newtype_struct() {
        #[derive(Serialize)]
        struct MyInt(u8);
        assert_eq!(to_bytes(&MyInt(6u8)).unwrap(), &[0x36]);
    }

    #[test]
    fn array_empty() {
        let a: [u32; 0] = [];
        assert_eq!(to_bytes(&a).unwrap(), &[0x01]);
    }

    #[test]
    fn array_no_index() {
        let a = [1, 2, 3];
        assert_eq!(to_bytes(&a).unwrap(), &[0x02, 0x05, 0x31, 0x32, 0x33]);

        let a = vec![1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1];
        let expected: Vec<u8> = vec![0x03, 0x02, 0x01, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31,
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
            0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31, 0x31];
        assert_eq!(to_bytes(&a).unwrap(), expected);

        let a: [[usize; 0]; 1] = [[]];
        assert_eq!(to_bytes(&a).unwrap(), &[0x02, 0x03, 0x01]);

        let a: [[usize; 1]; 1] = [[1]];
        assert_eq!(to_bytes(&a).unwrap(), &[0x02, 0x05, 0x02, 0x03, 0x31]);

        let a = vec![vec![vec![1,2,3],vec![4,5,6],vec![6,7,8]]];
        assert_eq!(to_bytes(&a).unwrap(), &[0x02, 0x13, 0x02, 0x11, 0x02, 0x05, 0x31, 0x32, 0x33, 0x02, 0x05, 0x34, 0x35, 0x36, 0x02, 0x05,
            0x36, 0x37, 0x38]);
    }

    #[test]
    fn array_with_index() {
        let a = &[1, 256];
        assert_eq!(to_bytes(&a).unwrap(), &[0x06, 0x09, 0x02, 0x31, 0x29, 0x00, 0x01, 0x03, 0x04]);

        let a = json!([1, "a"]);
        assert_eq!(to_bytes(&a).unwrap(), &[0x06, 0x08, 0x02, 0x31, 0x41, 0x61, 0x03, 0x04]);
    }

    #[test]
    fn object_empty() {
        let a: HashMap<i32, String> = HashMap::new();
        assert_eq!(to_bytes(&a).unwrap(), &[0x0a]);

        let a = json!({});
        assert_eq!(to_bytes(&a).unwrap(), &[0x0a]);
    }

    #[test]
    fn object() {
        let a = json!({"a": 1, "b": 2});
        assert_eq!(to_bytes(&a).unwrap(), &[0x0b, 0x0b, 0x02, 0x41, 0x61, 0x31, 0x41, 0x62, 0x32, 0x03, 0x06]);

        let a = json!({"a": 12, "b": true, "c": "xyz"});
        assert_eq!(to_bytes(&a).unwrap(), &[0x0b, 0x13, 0x03, 0x41, 0x61, 0x28, 0x0c, 0x41, 0x62, 0x1a, 0x41, 0x63, 0x43, 0x78, 0x79, 0x7a, 0x03, 0x07, 0x0a]);

        let a = json!({"b": true, "a": false});
        let expected: Vec<u8> = vec![0x0b, 0x0b, 0x02, 0x41, 0x61, 0x19, 0x41, 0x62, 0x1a, 0x03, 0x06];
        assert_eq!(to_bytes(&a).unwrap(), expected);

        #[derive(Serialize)]
        struct Person {
            name: String,
            age: u8,
            friends: Vec<Person>,
        }

        let p = Person {
            name: "Bob".to_owned(),
            age: 23,
            friends: vec![Person { name: "Alice".to_owned(), age: 42, friends: Vec::new() }]
        };
        println!("{:x?}", to_bytes(&p).unwrap());
        let expected: Vec<u8> = vec![0x0b, 0x3f, 0x03, 0x44, 0x6e, 0x61, 0x6d, 0x65, 0x43, 0x42, 0x6f, 0x62, 0x43, 0x61, 0x67, 0x65, 0x28, 0x17, 0x47, 0x66, 0x72, 0x69, 0x65, 0x6e, 0x64, 0x73, 0x02, 0x22, 0x0b, 0x20, 0x03, 0x44, 0x6e, 0x61, 0x6d, 0x65, 0x45, 0x41, 0x6c, 0x69, 0x63, 0x65, 0x43, 0x61, 0x67, 0x65, 0x28, 0x2a, 0x47, 0x66, 0x72, 0x69, 0x65, 0x6e, 0x64, 0x73, 0x01, 0x0e, 0x03, 0x14, 0x0c, 0x03, 0x12];
        assert_eq!(to_bytes(&p).unwrap(), expected);
    }
}
