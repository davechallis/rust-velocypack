# rust-velocypack

Rust implementation of the [VelocyPack](https://github.com/arangodb/velocypack)
protocol used by [ArangoDB](https://www.arangodb.com/) that uses
[serde](https://serde.rs/) for serialization/deserialization.


## Status

Currently a very early work in progress, with partial implementations of
both serialization and deserialization, with little to no documentation.

Currently (de)serialization for the following types has been implemented
(✓ = implemented, blank = not implemented yet, ✗ = no implementation
planned yet):

```
Serialize  Deserialize  Value type
✗          ✗            0x00 : none
✓                       0x01 : empty array
✓                       0x02 : array without index table, 1-byte byte length
✓                       0x03 : array without index table, 2-byte byte length
✓                       0x04 : array without index table, 4-byte byte length
✓                       0x05 : array without index table, 8-byte byte length
✓                       0x06 : array with 1-byte index table offsets, bytelen and # subvals
✓                       0x07 : array with 2-byte index table offsets, bytelen and # subvals
✓                       0x08 : array with 4-byte index table offsets, bytelen and # subvals
✓                       0x09 : array with 8-byte index table offsets, bytelen and # subvals
✓                       0x0a : empty object
✓                       0x0b : object with 1-byte index table offsets, sorted by attribute name, 1-byte bytelen and # subvals
✓                       0x0c : object with 2-byte index table offsets, sorted by attribute name, 2-byte bytelen and # subvals
✓                       0x0d : object with 4-byte index table offsets, sorted by attribute name, 4-byte bytelen and # subvals
✓                       0x0e : object with 8-byte index table offsets, sorted by attribute name, 8-byte bytelen and # subvals
✗                       0x0f : object with 1-byte index table offsets, not sorted by attribute name, 1-byte bytelen and # subvals
✗                       0x10 : object with 2-byte index table offsets, not sorted by attribute name, 2-byte bytelen and # subvals
✗                       0x11 : object with 4-byte index table offsets, not sorted by attribute name, 4-byte bytelen and # subvals
✗                       0x12 : object with 8-byte index table offsets, not sorted by attribute name, 8-byte bytelen and # subvals
✗                       0x13 : compact array, no index table
✗                       0x14 : compact object, no index table
✗          ✗            0x15-0x16 : reserved
✗          ✗            0x17 : illegal
✓          ✓            0x18 : null
✓          ✓            0x19 : false
✓          ✓            0x1a : true
✓          ✓            0x1b : double IEEE-754
                        0x1c : UTC-date
✗          ✗            0x1d : external (only in memory)
✗          ✗            0x1e : minKey
✗          ✗            0x1f : maxKey
✓          ✓            0x20-0x27 : signed int
✓          ✓            0x28-0x2f : uint
✓          ✓            0x30-0x39 : small integers
✓          ✓            0x3a-0x3f : small negative integers
✓          ✓            0x40-0xbe : UTF-8-string
✓                       0xbf : long UTF-8-string
✓                       0xc0-0xc7 : binary blob
✗          ✗            0xc8-0xcf : positive long packed BCD-encoded float
✗          ✗            0xd0-0xd7 : negative long packed BCD-encoded float
✗          ✗            0xd8-0xef : reserved
✗          ✗            0xf0-0xff : custom types
```

## Example

`Cargo.toml`:

```
[dependencies]
velocypack = "0.1.0"
serde = { version = "1.0", features = ["derive"] }
```

`src/main.rs`:

```rust
use serde::Serialize;

#[derive(Serialize)]
struct Person {
    name: String,
    age: u8,
    friends: Vec<Person>,
}

fn main() {
    let p = Person {
        name: "Bob".to_owned(),
        age: 23,
        friends: vec![
            Person {
                name: "Alice".to_owned(),
                age: 42,
                friends: Vec::new()
            }
        ]
    };
    println!("{:#04x?}", velocypack::to_bytes(&p).unwrap());
}
```

Output can be checked using the
[VelocyPack tools](https://github.com/arangodb/velocypack/tree/master/tools),
e.g.:

```
$ cargo run > /tmp/bob.vpack && vpack-to-json --hex /tmp/bob.vpack /tmp/bob.json && cat /tmp/bob.json
Successfully converted JSON infile '/tmp/bob.vpack'
VPack Infile size: 63
JSON Outfile size: 137
{
  "age" : 23,
  "name" : "Bob",
  "friends" : [
    {
      "age" : 42,
      "name" : "Alice",
      "friends" : [
      ]
    }
  ]
}
```
