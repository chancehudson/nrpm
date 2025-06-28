use serde::Deserialize;
use serde::Serialize;

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct UserModel {
    pub id: String,
    pub username: String,
    pub created_at: u64,

    pub password_hash: String,
}

impl UserModel {}

#[cfg(feature = "server")]
impl redb::Value for UserModel {
    type SelfType<'a> = UserModel;
    type AsBytes<'a> = Vec<u8>;

    fn fixed_width() -> Option<usize> {
        None // Variable width due to strings
    }

    fn from_bytes<'a>(data: &'a [u8]) -> Self::SelfType<'a>
    where
        Self: 'a,
    {
        bincode::deserialize(data).expect("Failed to deserialize User")
    }

    fn as_bytes<'a, 'b: 'a>(value: &'a Self::SelfType<'b>) -> Self::AsBytes<'a> {
        bincode::serialize(value).expect("Failed to serialize User")
    }

    fn type_name() -> redb::TypeName {
        redb::TypeName::new("User")
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct UserModelSafe {
    pub id: String,
    pub username: String,
    pub created_at: u64,
}

impl From<UserModel> for UserModelSafe {
    fn from(
        UserModel {
            id,
            username,
            created_at,
            password_hash: _,
        }: UserModel,
    ) -> Self {
        UserModelSafe {
            id,
            username,
            created_at,
        }
    }
}
