mod de;
mod error;
mod ser;

pub use de::{from_bytes, first_from_bytes, Deserializer};
pub use error::{Error, Result};
pub use ser::{to_bytes, Serializer};

pub(crate) const U8_SIZE: usize = std::mem::size_of::<u8>();
pub(crate) const U16_SIZE: usize = std::mem::size_of::<u16>();
pub(crate) const U32_SIZE: usize = std::mem::size_of::<u32>();
pub(crate) const U64_SIZE: usize = std::mem::size_of::<u64>();
