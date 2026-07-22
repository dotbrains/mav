use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Copy)]
pub enum LogOrder {
    #[default]
    DateOrder,
    TopoOrder,
    AuthorDateOrder,
    ReverseChronological,
}

impl LogOrder {
    pub fn as_arg(&self) -> &'static str {
        match self {
            LogOrder::DateOrder => "--date-order",
            LogOrder::TopoOrder => "--topo-order",
            LogOrder::AuthorDateOrder => "--author-date-order",
            LogOrder::ReverseChronological => "--reverse",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum LogSource {
    #[default]
    All,
    Branch(SharedString),
    Sha(Oid),
    Path(RepoPath),
}

impl LogSource {
    pub(super) fn get_args(&self) -> Result<Vec<&str>> {
        match self {
            LogSource::All => Ok(vec![
                "--ignore-missing", // needed in case of unborn HEAD
                "--branches",
                "--remotes",
                "--tags",
                "HEAD",
            ]),
            LogSource::Branch(branch) => Ok(vec![branch.as_str()]),
            LogSource::Sha(oid) => Ok(vec![
                str::from_utf8(oid.as_bytes()).context("Failed to build str from sha")?,
            ]),
            LogSource::Path(path) => Ok(vec!["--follow", "--", path.as_unix_str()]),
        }
    }
}

pub struct SearchCommitArgs {
    pub query: SharedString,
    pub case_sensitive: bool,
}

pub fn commit_hash_search_query(query: &str) -> Option<&str> {
    let query = query.trim();
    (7..=40)
        .contains(&query.len())
        .then_some(query)
        .filter(|query| query.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

pub fn delete_branch_flag(is_remote_tracking_ref: bool, force: bool) -> &'static str {
    match (is_remote_tracking_ref, force) {
        (true, true) => "-Dr",
        (true, false) => "-dr",
        (false, true) => "-D",
        (false, false) => "-d",
    }
}
