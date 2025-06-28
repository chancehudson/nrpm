use std::str::FromStr;

use serde::Deserialize;
use serde::Serialize;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HashId {
    bytes: [u8; 32],
}

impl From<blake3::Hash> for HashId {
    fn from(value: blake3::Hash) -> Self {
        Self {
            bytes: *value.as_bytes(),
        }
    }
}

impl FromStr for HashId {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self {
            bytes: hex::decode(s)?
                .try_into()
                .map_err(|_| anyhow::anyhow!("Invalid length: expected 32 bytes"))?,
        })
    }
}

impl ToString for HashId {
    fn to_string(&self) -> String {
        hex::encode(self.bytes)
    }
}

#[cfg(feature = "server")]
impl redb::Key for HashId {
    fn compare(data1: &[u8], data2: &[u8]) -> std::cmp::Ordering {
        data1.cmp(data2)
    }
}

#[cfg(feature = "server")]
impl redb::Value for HashId {
    type SelfType<'a> = HashId;
    type AsBytes<'a> = Vec<u8>;

    fn fixed_width() -> Option<usize> {
        Some(32)
    }

    fn from_bytes<'a>(data: &'a [u8]) -> Self::SelfType<'a>
    where
        Self: 'a,
    {
        Self {
            bytes: data.try_into().unwrap(),
        }
    }

    fn as_bytes<'a, 'b: 'a>(value: &'a Self::SelfType<'b>) -> Self::AsBytes<'a> {
        value.bytes.to_vec()
    }

    fn type_name() -> redb::TypeName {
        redb::TypeName::new("PackageVersionModel")
    }
}
