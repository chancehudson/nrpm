use anyhow::Result;
use serde::Deserialize;
use serde::Serialize;
#[cfg(feature = "server")]
use tokio::io::AsyncRead;

use super::*;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct PackageVersionModel {
    pub id: HashId,
    pub name: String,
    pub author_id: String,
    pub package_id: String,
    pub created_at: u64,
}

#[cfg(feature = "server")]
impl PackageVersionModel {
    pub async fn reader_by_id(storage: OnyxStorage, version_id: HashId) -> Result<impl AsyncRead> {
        storage.reader_async(&version_id.to_string()).await
    }
}

#[cfg(feature = "server")]
impl redb::Value for PackageVersionModel {
    type SelfType<'a> = PackageVersionModel;
    type AsBytes<'a> = Vec<u8>;

    fn fixed_width() -> Option<usize> {
        None // Variable width due to strings
    }

    fn from_bytes<'a>(data: &'a [u8]) -> Self::SelfType<'a>
    where
        Self: 'a,
    {
        bincode::deserialize(data).expect("Failed to deserialize PackageVersionModel")
    }

    fn as_bytes<'a, 'b: 'a>(value: &'a Self::SelfType<'b>) -> Self::AsBytes<'a> {
        bincode::serialize(value).expect("Failed to serialize PackageVersionModel")
    }

    fn type_name() -> redb::TypeName {
        redb::TypeName::new("PackageVersionModel")
    }
}
