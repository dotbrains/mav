use anyhow::{Context as _, Result};
use sqlez::{
    bindable::{Bind, Column, StaticColumnCount},
    statement::Statement,
};

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    Hash,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize,
)]
pub struct WorkspaceId(pub(crate) i64);

impl WorkspaceId {
    pub fn from_i64(value: i64) -> Self {
        Self(value)
    }
}

impl StaticColumnCount for WorkspaceId {}

impl Bind for WorkspaceId {
    fn bind(&self, statement: &Statement, start_index: i32) -> Result<i32> {
        self.0.bind(statement, start_index)
    }
}

impl Column for WorkspaceId {
    fn column(statement: &mut Statement, start_index: i32) -> Result<(Self, i32)> {
        i64::column(statement, start_index)
            .map(|(i, next_index)| (Self(i), next_index))
            .with_context(|| format!("Failed to read WorkspaceId at index {start_index}"))
    }
}

impl From<WorkspaceId> for i64 {
    fn from(val: WorkspaceId) -> Self {
        val.0
    }
}
