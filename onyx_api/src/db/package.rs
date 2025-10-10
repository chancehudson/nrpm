#[cfg(feature = "server")]
use std::sync::Arc;

#[cfg(feature = "server")]
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

    pub fn version(
        db: Arc<Database>,
        name: &str,
        version_name: &str,
    ) -> Result<Option<PackageVersionModel>> {
        let read = db.begin_read()?;
        let package_name_table = read.open_table(PACKAGE_NAME_TABLE)?;
        let package_version_name_table = read.open_table(PACKAGE_VERSION_NAME_TABLE)?;
        let version_table = read.open_table(VERSION_TABLE)?;
        if let Some(package_id) = package_name_table.get(name)?
            && let Some(version_id) =
                package_version_name_table.get((package_id.value(), version_name))?
            && let Some(version) = version_table.get(version_id.value())?
        {
            Ok(Some(version.value()))
        } else {
            Ok(None)
        }
    }

    pub fn versions(
        db: Arc<Database>,
        name: &str,
    ) -> Result<Option<(PackageModel, Vec<PackageVersionModel>)>> {
        let read = db.begin_read()?;
        let package_name_table = read.open_table(PACKAGE_NAME_TABLE)?;
        let package_table = read.open_table(PACKAGE_TABLE)?;
        let package_version_table = read.open_multimap_table(PACKAGE_VERSION_TABLE)?;
        let version_table = read.open_table(VERSION_TABLE)?;
        if let Some(package_id) = package_name_table.get(name)?
            && let Some(package) = package_table.get(package_id.value())?
        {
            let version_ids = package_version_table.get(package_id.value())?;
            let versions = version_ids
                .into_iter()
                .map(|version_id|
                    {
                        let version_id = version_id?.value();
                        Ok((version_table.get(&version_id)?, version_id))
                    })
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .filter_map(|(version_maybe, version_id)| {
                    if version_maybe.is_none() {
                            log::warn!("version \"{}\" does not exist for package \"{}\". PACKAGE_VERSION_TABLE and VERSION_TABLE are inconsistent!", version_id.to_string(), package.value().name);
                    }
                    version_maybe.map(|v| v.value())
                })
            .collect::<Vec<_>>();
            Ok(Some((package.value(), versions)))
        } else {
            Ok(None)
        }
    }

    pub fn latest_version(
        db: Arc<Database>,
        name: &str,
    ) -> Result<Option<(PackageModel, PackageVersionModel)>> {
        let read = db.begin_read()?;
        let package_table = read.open_table(PACKAGE_TABLE)?;
        let package_name_table = read.open_table(PACKAGE_NAME_TABLE)?;
        let version_table = read.open_table(VERSION_TABLE)?;
        if let Some(package_id) = package_name_table.get(name)?
            && let Some(package) = package_table.get(package_id.value())?
            && let Some(version) = version_table.get(package.value().latest_version_id)?
        {
            Ok(Some((package.value(), version.value())))
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
