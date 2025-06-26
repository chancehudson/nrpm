use serde::Deserialize;
use serde::Serialize;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct PackageVersionModel {
    pub id: String,
    pub name: String,
    pub author_id: String,
    pub package_id: String,
    pub hash: [u8; 32],
    pub created_at: u64,
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
