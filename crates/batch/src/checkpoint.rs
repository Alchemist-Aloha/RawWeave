use crate::{BatchError, CheckpointPolicy};
use rawweave_graph::{
    ArtifactStore, Checkpoint, CheckpointArtifact, CheckpointAvailability, CheckpointError,
    GenerationToken,
};
use std::collections::BTreeMap;

/// The action a batch item should take for one manual checkpoint.
#[derive(Clone, Debug, PartialEq)]
pub enum CheckpointResolution {
    /// Reuse the committed artifact. A stale artifact is permitted by all
    /// policies that return this variant and is marked for diagnostics.
    UseCommitted {
        artifact: Box<CheckpointArtifact>,
        stale: bool,
    },
    /// Generate a new artifact and complete it with the returned token.
    Generate { token: GenerationToken },
}

/// Resolves explicit batch checkpoint policy against persisted graph state.
///
/// The runtime owns only the in-memory checkpoint map for the current
/// workflow. The artifact store is shared with the graph and may be memory or
/// file backed; generation state is deliberately tokenized so cancellation,
/// failures, and late results cannot commit an unrelated attempt.
pub struct CheckpointRuntime<'a> {
    policy: CheckpointPolicy,
    checkpoints: &'a mut BTreeMap<String, Checkpoint>,
    store: &'a ArtifactStore,
}

impl<'a> CheckpointRuntime<'a> {
    pub fn new(
        policy: CheckpointPolicy,
        checkpoints: &'a mut BTreeMap<String, Checkpoint>,
        store: &'a ArtifactStore,
    ) -> Self {
        Self {
            policy,
            checkpoints,
            store,
        }
    }

    pub fn policy(&self) -> CheckpointPolicy {
        self.policy
    }

    pub fn checkpoints(&self) -> &BTreeMap<String, Checkpoint> {
        self.checkpoints
    }

    pub fn checkpoint(&self, node_id: &str) -> Result<&Checkpoint, BatchError> {
        self.checkpoints.get(node_id).ok_or_else(|| {
            BatchError::CheckpointPolicy(format!(
                "checkpoint '{node_id}' is not registered in the pinned workflow"
            ))
        })
    }

    pub fn resolve(&mut self, node_id: &str) -> Result<CheckpointResolution, BatchError> {
        let policy = self.policy;
        let store = self.store;
        let checkpoint = self.checkpoints.get_mut(node_id).ok_or_else(|| {
            BatchError::CheckpointPolicy(format!(
                "checkpoint '{node_id}' is not registered in the pinned workflow"
            ))
        })?;
        if checkpoint.node_id != node_id {
            return Err(Self::policy_error(
                policy,
                node_id,
                "the checkpoint provenance belongs to a different node",
            ));
        }
        let availability = checkpoint.availability_with_store(store)?;
        match policy {
            CheckpointPolicy::UseCommitted => {
                Self::use_committed(policy, node_id, checkpoint, availability, store)
            }
            CheckpointPolicy::GenerateIfMissing => match availability {
                CheckpointAvailability::Fresh | CheckpointAvailability::Stale => {
                    Self::use_committed(policy, node_id, checkpoint, availability, store)
                }
                CheckpointAvailability::Missing | CheckpointAvailability::Incompatible => {
                    Self::begin_generation(policy, node_id, checkpoint)
                }
            },
            CheckpointPolicy::RegenerateAll => Self::begin_generation(policy, node_id, checkpoint),
            CheckpointPolicy::FailIfStale => match availability {
                CheckpointAvailability::Fresh => {
                    Self::use_committed(policy, node_id, checkpoint, availability, store)
                }
                CheckpointAvailability::Stale => Err(Self::policy_error(
                    policy,
                    node_id,
                    "the committed artifact is stale; regenerate it or select use_committed",
                )),
                CheckpointAvailability::Missing => Err(Self::policy_error(
                    policy,
                    node_id,
                    "no committed artifact is available; generate the checkpoint first",
                )),
                CheckpointAvailability::Incompatible => Err(Self::policy_error(
                    policy,
                    node_id,
                    "the committed artifact is incompatible with this checkpoint; regenerate it",
                )),
            },
            policy => Err(Self::policy_error(
                policy,
                node_id,
                &format!(
                    "policy {policy:?} is a job-progress mode, not an explicit checkpoint policy"
                ),
            )),
        }
    }

    pub fn commit_generation(
        &mut self,
        node_id: &str,
        token: GenerationToken,
        artifact: CheckpointArtifact,
    ) -> Result<(), BatchError> {
        let checkpoint = self.checkpoints.get_mut(node_id).ok_or_else(|| {
            BatchError::CheckpointPolicy(format!(
                "checkpoint '{node_id}' disappeared before generation commit"
            ))
        })?;
        checkpoint
            .commit_generation(token, artifact, self.store)
            .map_err(BatchError::from)
    }

    pub fn cancel_generation(
        &mut self,
        node_id: &str,
        token: GenerationToken,
    ) -> Result<(), BatchError> {
        let checkpoint = self.checkpoints.get_mut(node_id).ok_or_else(|| {
            BatchError::CheckpointPolicy(format!(
                "checkpoint '{node_id}' disappeared before generation cancellation"
            ))
        })?;
        checkpoint
            .cancel_generation(&token)
            .map_err(BatchError::from)
    }

    pub fn fail_generation(
        &mut self,
        node_id: &str,
        token: GenerationToken,
        message: impl Into<String>,
    ) -> Result<(), BatchError> {
        let checkpoint = self.checkpoints.get_mut(node_id).ok_or_else(|| {
            BatchError::CheckpointPolicy(format!(
                "checkpoint '{node_id}' disappeared before generation failure was recorded"
            ))
        })?;
        checkpoint
            .fail_generation(&token, message)
            .map_err(BatchError::from)
    }

    fn use_committed(
        policy: CheckpointPolicy,
        node_id: &str,
        checkpoint: &Checkpoint,
        availability: CheckpointAvailability,
        store: &ArtifactStore,
    ) -> Result<CheckpointResolution, BatchError> {
        if availability == CheckpointAvailability::Incompatible {
            return Err(Self::policy_error(
                policy,
                node_id,
                "the committed artifact is incompatible with this checkpoint; regenerate it",
            ));
        }
        let artifact = checkpoint
            .committed_artifact(store)?
            .ok_or_else(|| match availability {
                CheckpointAvailability::Incompatible => Self::policy_error(
                    policy,
                    node_id,
                    "the committed artifact is incompatible with this checkpoint; regenerate it",
                ),
                _ => Self::policy_error(
                    policy,
                    node_id,
                    "no committed artifact is available; generate the checkpoint first",
                ),
            })?;
        Ok(CheckpointResolution::UseCommitted {
            artifact: Box::new(artifact),
            stale: availability == CheckpointAvailability::Stale,
        })
    }

    fn begin_generation(
        policy: CheckpointPolicy,
        node_id: &str,
        checkpoint: &mut Checkpoint,
    ) -> Result<CheckpointResolution, BatchError> {
        let token = checkpoint.begin_generation_token().map_err(|error| {
            if matches!(error, CheckpointError::AlreadyGenerating) {
                Self::policy_error(
                    policy,
                    node_id,
                    "a generation is already in progress; wait for it to finish or cancel it",
                )
            } else {
                BatchError::from(error)
            }
        })?;
        Ok(CheckpointResolution::Generate { token })
    }

    fn policy_error(policy: CheckpointPolicy, node_id: &str, reason: &str) -> BatchError {
        BatchError::CheckpointPolicy(format!(
            "checkpoint '{node_id}' under policy {:?}: {reason}",
            policy
        ))
    }
}
