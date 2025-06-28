use std::io::Read;

use serde::Deserialize;
use serde::Serialize;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct PackageModel {
    pub id: String,
    pub name: String,
    pub author_id: String,
    pub latest_version_id: String,
}

// impl PackageModel { pub async fn download<T>(&self, package_id: &str, version_id: &str) -> Result<T>
//     where
//         T: Read,
//     {
//     }
// }

#[cfg(feature = "server")]
impl redb::Value for PackageModel {
    type SelfType<'a> = PackageModel;
    type AsBytes<'a> = Vec<u8>;

    fn fixed_width() -> Option<usize> {
        None // Variable width due to strings
    }

    fn from_bytes<'a>(data: &'a [u8]) -> Self::SelfType<'a>
    where
        Self: 'a,
    {
        bincode::deserialize(data).expect("Failed to deserialize PackageModel")
    }

    fn as_bytes<'a, 'b: 'a>(value: &'a Self::SelfType<'b>) -> Self::AsBytes<'a> {
        bincode::serialize(value).expect("Failed to serialize PackageModel")
    }

    fn type_name() -> redb::TypeName {
        redb::TypeName::new("PackageModel")
    }
}
