use std::sync::Arc;

use anyhow::Result;
#[cfg(feature = "server")]
use redb::Database;
use serde::Deserialize;
use serde::Serialize;

use super::*;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct PackageModel {
    pub id: String,
    pub name: String,
    pub author_id: String,
    pub latest_version_id: HashId,
}

#[cfg(feature = "server")]
impl PackageModel {
    pub fn package_by_name(db: Arc<Database>, name: &str) -> Result<Option<Self>> {
        let read = db.begin_read()?;
        let package_table = read.open_table(PACKAGE_TABLE)?;
        let package_name_table = read.open_table(PACKAGE_NAME_TABLE)?;
        if let Some(package_id) = package_name_table.get(name)?
            && let Some(package) = package_table.get(package_id.value())?
        {
            Ok(Some(package.value()))
        } else {
            Ok(None)
        }
    }

    pub fn latest_version(db: Arc<Database>, name: &str) -> Result<Option<PackageVersionModel>> {
        let read = db.begin_read()?;
        let package_table = read.open_table(PACKAGE_TABLE)?;
        let package_name_table = read.open_table(PACKAGE_NAME_TABLE)?;
        let version_table = read.open_table(VERSION_TABLE)?;
        if let Some(package_id) = package_name_table.get(name)?
            && let Some(package) = package_table.get(package_id.value())?
            && let Some(version) = version_table.get(package.value().latest_version_id)?
        {
            Ok(Some(version.value()))
        } else {
            Ok(None)
        }
    }
}

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
