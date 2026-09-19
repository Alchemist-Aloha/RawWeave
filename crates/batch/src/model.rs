use crate::{BatchError, sha256_file};
use rawweave_graph::{NodePackDependency, SubgraphDependency, WorkflowDefinition};
use rawweave_node_api::ParameterValue;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

pub const BATCH_JOB_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PinnedWorkflow {
    pub definition: WorkflowDefinition,
    pub revision: u64,
    pub hash: String,
}

impl PinnedWorkflow {
    pub fn new(definition: WorkflowDefinition, revision: u64) -> Result<Self, BatchError> {
        definition
            .validate()
            .map_err(|error| BatchError::Workflow(error.to_string()))?;
        let hash = definition.hash();
        Ok(Self {
            definition,
            revision,
            hash,
        })
    }

    pub fn verify(&self) -> Result<(), BatchError> {
        let actual = self.definition.hash();
        if actual != self.hash {
            return Err(BatchError::WorkflowHashMismatch {
                expected: self.hash.clone(),
                actual,
            });
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PinnedDependencies {
    #[serde(default)]
    pub node_packs: Vec<NodePackDependency>,
    #[serde(default)]
    pub subgraphs: Vec<SubgraphDependency>,
    #[serde(default)]
    pub plugins: BTreeMap<String, String>,
    #[serde(default)]
    pub external_providers: BTreeMap<String, String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointPolicy {
    #[default]
    AfterEachItem,
    AfterEachOutput,
    Manual,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemState {
    #[default]
    Waiting,
    Running,
    Completed,
    Skipped,
    Failed,
    Cancelled,
}

impl ItemState {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Skipped | Self::Failed | Self::Cancelled
        )
    }

    pub fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (
                Self::Waiting,
                Self::Running | Self::Skipped | Self::Cancelled
            ) | (
                Self::Running,
                Self::Completed | Self::Failed | Self::Cancelled
            ) | (
                Self::Failed,
                Self::Waiting | Self::Skipped | Self::Cancelled
            ) | (Self::Skipped, Self::Waiting | Self::Cancelled)
                | (Self::Cancelled, Self::Waiting)
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputRecord {
    pub path: PathBuf,
    pub sha256: String,
    pub byte_len: u64,
}

impl OutputRecord {
    pub fn validate(&self) -> Result<bool, BatchError> {
        if !self.path.is_file() {
            return Ok(false);
        }
        let metadata = fs::metadata(&self.path).map_err(|error| BatchError::Io {
            operation: "inspect completed output",
            path: self.path.clone(),
            source: error,
        })?;
        if metadata.len() != self.byte_len {
            return Ok(false);
        }
        Ok(sha256_file(&self.path)? == self.sha256)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BatchItem {
    pub id: String,
    pub source_path: PathBuf,
    pub display_name: String,
    #[serde(default)]
    pub overrides: BTreeMap<String, ParameterValue>,
    #[serde(default)]
    pub test_set: bool,
    #[serde(default)]
    pub state: ItemState,
    #[serde(default)]
    pub attempts: u32,
    #[serde(default)]
    pub failure: Option<String>,
    #[serde(default)]
    pub outputs: Vec<OutputRecord>,
}

impl BatchItem {
    pub fn new(
        id: impl Into<String>,
        source_path: impl Into<PathBuf>,
        display_name: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            source_path: source_path.into(),
            display_name: display_name.into(),
            overrides: BTreeMap::new(),
            test_set: false,
            state: ItemState::Waiting,
            attempts: 0,
            failure: None,
            outputs: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchState {
    #[default]
    Draft,
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    Jpeg,
    Png,
    Tiff,
    OpenExr,
}

impl OutputFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Png => "png",
            Self::Tiff => "tif",
            Self::OpenExr => "exr",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Resolution {
    #[default]
    Original,
    Exact {
        width: u32,
        height: u32,
    },
    LongEdge(u32),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BitDepth {
    #[default]
    Eight,
    Sixteen,
    Float32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorSpace {
    LinearSrgb,
    #[default]
    Srgb,
    DisplayP3,
    Named(String),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetadataPolicy {
    #[default]
    Preserve,
    Strip,
    Sidecar,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum OutputSharpening {
    #[default]
    None,
    UnsharpMask {
        radius: u32,
        amount: f32,
        threshold: f32,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Compression {
    #[default]
    Default,
    Fast,
    Best,
    Lossless,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollisionPolicy {
    Error,
    Skip,
    Overwrite,
    #[default]
    Suffix,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OutputRecipe {
    pub format: OutputFormat,
    #[serde(default)]
    pub resolution: Resolution,
    #[serde(default)]
    pub bit_depth: BitDepth,
    #[serde(default)]
    pub color_space: ColorSpace,
    #[serde(default)]
    pub icc_profile: Option<PathBuf>,
    #[serde(default)]
    pub ocio_transform: Option<String>,
    #[serde(default)]
    pub metadata_policy: MetadataPolicy,
    #[serde(default)]
    pub sharpening: OutputSharpening,
    #[serde(default)]
    pub quality: u8,
    #[serde(default)]
    pub compression: Compression,
    pub destination: PathBuf,
    pub filename_template: String,
    #[serde(default)]
    pub collision_policy: CollisionPolicy,
}

impl OutputRecipe {
    pub fn new(format: OutputFormat, destination: impl Into<PathBuf>) -> Self {
        Self {
            format,
            resolution: Resolution::Original,
            bit_depth: BitDepth::Eight,
            color_space: ColorSpace::Srgb,
            icc_profile: None,
            ocio_transform: None,
            metadata_policy: MetadataPolicy::Preserve,
            sharpening: OutputSharpening::None,
            quality: 92,
            compression: Compression::Default,
            destination: destination.into(),
            filename_template: "{stem}-{index}".to_owned(),
            collision_policy: CollisionPolicy::Suffix,
        }
    }

    pub fn with_filename_template(mut self, template: impl Into<String>) -> Self {
        self.filename_template = template.into();
        self
    }

    pub fn with_resolution(mut self, resolution: Resolution) -> Self {
        self.resolution = resolution;
        self
    }

    pub fn validate(&self) -> Result<(), BatchError> {
        if self.destination.as_os_str().is_empty() {
            return Err(BatchError::InvalidRecipe(
                "destination cannot be empty".to_owned(),
            ));
        }
        if self.filename_template.trim().is_empty() {
            return Err(BatchError::InvalidRecipe(
                "filename template cannot be empty".to_owned(),
            ));
        }
        if self.filename_template.contains('/') || self.filename_template.contains('\\') {
            return Err(BatchError::InvalidRecipe(
                "filename template must produce a single file name".to_owned(),
            ));
        }
        if !matches!(self.quality, 1..=100) && self.format == OutputFormat::Jpeg {
            return Err(BatchError::InvalidRecipe(
                "JPEG quality must be between 1 and 100".to_owned(),
            ));
        }
        if matches!(
            self.resolution,
            Resolution::Exact { width: 0, .. }
                | Resolution::Exact { height: 0, .. }
                | Resolution::LongEdge(0)
        ) {
            return Err(BatchError::InvalidRecipe(
                "output resolution must be greater than zero".to_owned(),
            ));
        }
        if self.format == OutputFormat::Jpeg && self.bit_depth != BitDepth::Eight {
            return Err(BatchError::UnsupportedBitDepth {
                format: self.format,
                bit_depth: self.bit_depth,
            });
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DryRunSubset {
    CurrentPreview {
        item_id: String,
    },
    TestSet,
    FirstN(usize),
    Selected(Vec<String>),
    #[default]
    All,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BatchJob {
    pub schema_version: u32,
    pub id: String,
    pub workflow: PinnedWorkflow,
    pub dependencies: PinnedDependencies,
    #[serde(default)]
    pub overrides: BTreeMap<String, ParameterValue>,
    pub recipes: Vec<OutputRecipe>,
    pub checkpoint_policy: CheckpointPolicy,
    pub items: Vec<BatchItem>,
    #[serde(default)]
    pub state: BatchState,
}

impl BatchJob {
    pub fn new(
        id: impl Into<String>,
        workflow: PinnedWorkflow,
        dependencies: PinnedDependencies,
        overrides: BTreeMap<String, ParameterValue>,
        recipes: Vec<OutputRecipe>,
        checkpoint_policy: CheckpointPolicy,
        items: Vec<BatchItem>,
    ) -> Result<Self, BatchError> {
        let job = Self {
            schema_version: BATCH_JOB_SCHEMA_VERSION,
            id: id.into(),
            workflow,
            dependencies,
            overrides,
            recipes,
            checkpoint_policy,
            items,
            state: BatchState::Draft,
        };
        job.validate()?;
        Ok(job)
    }

    pub fn validate(&self) -> Result<(), BatchError> {
        if self.schema_version != BATCH_JOB_SCHEMA_VERSION {
            return Err(BatchError::UnsupportedSchema(self.schema_version));
        }
        if self.id.trim().is_empty() {
            return Err(BatchError::InvalidJob("job id cannot be empty".to_owned()));
        }
        self.workflow.verify()?;
        if self.recipes.is_empty() {
            return Err(BatchError::InvalidJob(
                "at least one output recipe is required".to_owned(),
            ));
        }
        for recipe in &self.recipes {
            recipe.validate()?;
        }
        let mut ids = BTreeSet::new();
        for item in &self.items {
            if item.id.trim().is_empty() || !ids.insert(&item.id) {
                return Err(BatchError::InvalidJob(format!(
                    "batch item id '{}' is empty or duplicated",
                    item.id
                )));
            }
        }
        Ok(())
    }

    pub fn selected_items(&self, subset: &DryRunSubset) -> Result<Vec<&BatchItem>, BatchError> {
        match subset {
            DryRunSubset::CurrentPreview { item_id } => self
                .items
                .iter()
                .filter(|item| item.id == *item_id)
                .map(Some)
                .next()
                .flatten()
                .map(|item| vec![item])
                .ok_or_else(|| BatchError::UnknownItem(item_id.clone())),
            DryRunSubset::TestSet => Ok(self.items.iter().filter(|item| item.test_set).collect()),
            DryRunSubset::FirstN(count) => Ok(self.items.iter().take(*count).collect()),
            DryRunSubset::Selected(ids) => {
                let by_id = self
                    .items
                    .iter()
                    .map(|item| (item.id.as_str(), item))
                    .collect::<BTreeMap<_, _>>();
                ids.iter()
                    .map(|id| {
                        by_id
                            .get(id.as_str())
                            .copied()
                            .ok_or_else(|| BatchError::UnknownItem(id.clone()))
                    })
                    .collect()
            }
            DryRunSubset::All => Ok(self.items.iter().collect()),
        }
    }

    pub fn transition_item(&mut self, id: &str, next: ItemState) -> Result<(), BatchError> {
        let item = self
            .items
            .iter_mut()
            .find(|item| item.id == id)
            .ok_or_else(|| BatchError::UnknownItem(id.to_owned()))?;
        if !item.state.can_transition_to(next) {
            return Err(BatchError::InvalidTransition {
                item_id: id.to_owned(),
                from: item.state,
                to: next,
            });
        }
        item.state = next;
        if next == ItemState::Running {
            item.attempts = item.attempts.saturating_add(1);
            item.failure = None;
        }
        if next != ItemState::Failed {
            item.failure = None;
        }
        self.refresh_state();
        Ok(())
    }

    pub fn fail_item(&mut self, id: &str, message: impl Into<String>) -> Result<(), BatchError> {
        let message = message.into();
        self.transition_item(id, ItemState::Failed)?;
        if let Some(item) = self.items.iter_mut().find(|item| item.id == id) {
            item.failure = Some(message);
        }
        self.refresh_state();
        Ok(())
    }

    pub fn complete_item(
        &mut self,
        id: &str,
        outputs: Vec<OutputRecord>,
    ) -> Result<(), BatchError> {
        self.transition_item(id, ItemState::Completed)?;
        if let Some(item) = self.items.iter_mut().find(|item| item.id == id) {
            item.outputs = outputs;
        }
        self.refresh_state();
        Ok(())
    }

    pub fn retry_failed(&mut self) -> Result<usize, BatchError> {
        let ids = self
            .items
            .iter()
            .filter(|item| item.state == ItemState::Failed)
            .map(|item| item.id.clone())
            .collect::<Vec<_>>();
        self.retry_ids(&ids)
    }

    pub fn retry_selected(&mut self, ids: &[String]) -> Result<usize, BatchError> {
        self.retry_ids(ids)
    }

    fn retry_ids(&mut self, ids: &[String]) -> Result<usize, BatchError> {
        let mut count = 0;
        for id in ids {
            let item = self
                .items
                .iter_mut()
                .find(|item| item.id == *id)
                .ok_or_else(|| BatchError::UnknownItem(id.clone()))?;
            if matches!(
                item.state,
                ItemState::Failed | ItemState::Cancelled | ItemState::Skipped
            ) {
                if !item.state.can_transition_to(ItemState::Waiting) {
                    return Err(BatchError::InvalidTransition {
                        item_id: item.id.clone(),
                        from: item.state,
                        to: ItemState::Waiting,
                    });
                }
                item.state = ItemState::Waiting;
                item.failure = None;
                item.outputs.clear();
                count += 1;
            }
        }
        self.refresh_state();
        Ok(count)
    }

    pub fn skip(&mut self, ids: &[String]) -> Result<usize, BatchError> {
        let mut count = 0;
        for id in ids {
            let item = self
                .items
                .iter_mut()
                .find(|item| item.id == *id)
                .ok_or_else(|| BatchError::UnknownItem(id.clone()))?;
            if item.state != ItemState::Skipped {
                if !item.state.can_transition_to(ItemState::Skipped) {
                    return Err(BatchError::InvalidTransition {
                        item_id: item.id.clone(),
                        from: item.state,
                        to: ItemState::Skipped,
                    });
                }
                item.state = ItemState::Skipped;
                item.failure = None;
                count += 1;
            }
        }
        self.refresh_state();
        Ok(count)
    }

    pub fn requeue_invalid_completed(&mut self) -> Result<usize, BatchError> {
        let completed = self
            .items
            .iter()
            .filter(|item| item.state == ItemState::Completed)
            .map(|item| item.id.clone())
            .collect::<Vec<_>>();
        let mut count = 0;
        for id in completed {
            let valid = self
                .items
                .iter()
                .find(|item| item.id == id)
                .map(|item| {
                    item.outputs
                        .iter()
                        .all(|output| output.validate().unwrap_or(false))
                })
                .unwrap_or(false);
            if !valid {
                let item = self
                    .items
                    .iter_mut()
                    .find(|item| item.id == id)
                    .ok_or_else(|| BatchError::UnknownItem(id.clone()))?;
                item.state = ItemState::Waiting;
                item.outputs.clear();
                count += 1;
            }
        }
        self.refresh_state();
        Ok(count)
    }

    pub fn validate_completed_outputs(&self) -> Result<bool, BatchError> {
        for item in &self.items {
            if item.state == ItemState::Completed
                && !item
                    .outputs
                    .iter()
                    .all(|output| output.validate().unwrap_or(false))
            {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub fn refresh_state(&mut self) {
        if self.items.is_empty() {
            self.state = BatchState::Completed;
            return;
        }
        if self
            .items
            .iter()
            .any(|item| item.state == ItemState::Running)
        {
            if self.state != BatchState::Paused {
                self.state = BatchState::Running;
            }
            return;
        }
        if self
            .items
            .iter()
            .any(|item| item.state == ItemState::Waiting)
        {
            if !matches!(self.state, BatchState::Draft | BatchState::Cancelled) {
                self.state = BatchState::Running;
            }
            return;
        }
        if self
            .items
            .iter()
            .any(|item| item.state == ItemState::Failed)
        {
            self.state = BatchState::Failed;
        } else if self
            .items
            .iter()
            .all(|item| item.state == ItemState::Cancelled)
        {
            self.state = BatchState::Cancelled;
        } else {
            self.state = BatchState::Completed;
        }
    }

    pub fn output_path(&self, recipe: &OutputRecipe, item: &BatchItem, index: usize) -> PathBuf {
        rendered_output_path(recipe, item, index)
    }
}

pub(crate) fn rendered_output_path(
    recipe: &OutputRecipe,
    item: &BatchItem,
    index: usize,
) -> PathBuf {
    let stem = item
        .display_name
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(&item.display_name);
    let mut filename = recipe.filename_template.clone();
    for (token, value) in [
        ("{name}", item.display_name.as_str()),
        ("{stem}", stem),
        ("{id}", item.id.as_str()),
    ] {
        filename = filename.replace(token, value);
    }
    filename = filename.replace("{index}", &index.to_string());
    let extension = recipe.format.extension();
    if Path::new(&filename).extension().is_none() {
        filename.push('.');
        filename.push_str(extension);
    }
    recipe.destination.join(filename)
}
