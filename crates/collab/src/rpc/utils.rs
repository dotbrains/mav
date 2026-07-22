use crate::entities::User;
use rpc::proto;

pub trait ResultExt {
    type Ok;

    fn trace_err(self) -> Option<Self::Ok>;
}

impl<T, E> ResultExt for Result<T, E>
where
    E: std::fmt::Debug,
{
    type Ok = T;

    #[track_caller]
    fn trace_err(self) -> Option<T> {
        match self {
            Ok(value) => Some(value),
            Err(error) => {
                tracing::error!("{:?}", error);
                None
            }
        }
    }
}

impl From<User> for proto::User {
    fn from(user: User) -> Self {
        Self {
            id: user.id.to_proto(),
            username: user.username,
            avatar_url: user.avatar_url,
            github_login: user.github_login,
            name: user.name,
        }
    }
}
