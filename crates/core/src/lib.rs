use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CoreError {
    #[error("node identifier cannot be empty")]
    EmptyNodeId,
}

/// Stable identity for a node instance. The value is persisted in workflows.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NodeId(String);

impl NodeId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn try_new(value: impl Into<String>) -> Result<Self, CoreError> {
        let value = value.into();
        if value.is_empty() {
            return Err(CoreError::EmptyNodeId);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for NodeId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for NodeId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::{CoreError, NodeId};

    #[test]
    fn node_ids_preserve_their_stable_value() {
        let id = NodeId::try_new("exposure").unwrap();
        assert_eq!(id.as_str(), "exposure");
        assert_eq!(id.to_string(), "exposure");
    }

    #[test]
    fn empty_node_ids_are_rejected_by_validating_constructor() {
        assert_eq!(NodeId::try_new(""), Err(CoreError::EmptyNodeId));
    }
}
