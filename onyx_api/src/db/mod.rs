mod hash_id;
mod package;
mod user;
mod version;

pub use hash_id::*;
pub use package::*;
pub use user::*;
pub use version::*;

use super::*;

#[cfg(feature = "server")]
pub mod tables {
    use super::*;

    use redb::MultimapTableDefinition;
    use redb::TableDefinition;

    type NanoId<'a> = &'a str;
    // auth token keyed to expiration timestamp
    pub const AUTH_TOKEN_TABLE: TableDefinition<NanoId, (NanoId, u64)> =
        TableDefinition::new("auth_tokens");
    // user_id keyed to user document
    pub const USER_TABLE: TableDefinition<NanoId, UserModel> = TableDefinition::new("users");
    // username keyed to user_id
    pub const USERNAME_USER_ID_TABLE: TableDefinition<&str, NanoId> =
        TableDefinition::new("username_user_id");

    pub const PACKAGE_TABLE: TableDefinition<NanoId, PackageModel> =
        TableDefinition::new("packages");
    // used to ensure package names are unique
    // TODO: sort by semver ordering for efficient latest version lookups
    pub const PACKAGE_NAME_TABLE: TableDefinition<&str, NanoId> =
        TableDefinition::new("package_names");
    // used to prevent multiple versions with the same name for a single package
    // (package_id, version_name) keyed to ()
    pub const PACKAGE_VERSION_NAME_TABLE: TableDefinition<(NanoId, &str), HashId> =
        TableDefinition::new("package_version_name");
    // package_id keyed to many versions
    pub const PACKAGE_VERSION_TABLE: MultimapTableDefinition<NanoId, HashId> =
        MultimapTableDefinition::new("package_versions");
    pub const VERSION_TABLE: TableDefinition<HashId, PackageVersionModel> =
        TableDefinition::new("versions");

    // a list of the refs for each version of a package
    // package_id keyed to refs in a single string
    pub const GIT_REFS_TABLE: TableDefinition<NanoId, &str> = TableDefinition::new("git_refs");
    // commit_id_hex keyed to pack bytes
    pub const GIT_PACK_TABLE: TableDefinition<&str, Vec<u8>> = TableDefinition::new("git_packs");
}

#[cfg(feature = "server")]
pub use tables::*;
