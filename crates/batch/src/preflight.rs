use crate::model::{BatchJob, CollisionPolicy, rendered_output_path};
use rawweave_graph::NodePackManifest;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub severity: DiagnosticSeverity,
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub item_id: Option<String>,
    #[serde(default)]
    pub path: Option<PathBuf>,
}

impl Diagnostic {
    fn new(
        severity: DiagnosticSeverity,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity,
            code: code.into(),
            message: message.into(),
            item_id: None,
            path: None,
        }
    }

    fn for_item(
        severity: DiagnosticSeverity,
        code: impl Into<String>,
        message: impl Into<String>,
        item_id: &str,
        path: Option<PathBuf>,
    ) -> Self {
        Self {
            severity,
            code: code.into(),
            message: message.into(),
            item_id: Some(item_id.to_owned()),
            path,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreflightReport {
    pub diagnostics: Vec<Diagnostic>,
}

impl PreflightReport {
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
    }

    pub fn errors(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
    }

    pub fn warnings(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == DiagnosticSeverity::Warning)
    }

    pub fn infos(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == DiagnosticSeverity::Info)
    }

    pub fn push(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreflightOptions {
    #[serde(default = "default_true")]
    pub check_source_files: bool,
    #[serde(default = "default_true")]
    pub check_output_directories: bool,
    #[serde(default = "default_true")]
    pub check_collisions: bool,
    #[serde(default)]
    pub available_node_packs: Vec<NodePackManifest>,
    #[serde(default)]
    pub available_subgraphs: BTreeMap<String, String>,
    #[serde(default)]
    pub available_plugins: BTreeMap<String, String>,
    #[serde(default)]
    pub available_external_providers: BTreeMap<String, String>,
    #[serde(default)]
    pub available_disk_bytes: Option<u64>,
}

fn default_true() -> bool {
    true
}

impl Default for PreflightOptions {
    fn default() -> Self {
        Self {
            check_source_files: true,
            check_output_directories: true,
            check_collisions: true,
            available_node_packs: Vec::new(),
            available_subgraphs: BTreeMap::new(),
            available_plugins: BTreeMap::new(),
            available_external_providers: BTreeMap::new(),
            available_disk_bytes: None,
        }
    }
}

impl PreflightOptions {
    pub fn with_node_packs(mut self, manifests: Vec<NodePackManifest>) -> Self {
        self.available_node_packs = manifests;
        self
    }

    pub fn with_subgraph(mut self, id: impl Into<String>, hash: impl Into<String>) -> Self {
        self.available_subgraphs.insert(id.into(), hash.into());
        self
    }

    pub fn with_plugin(mut self, id: impl Into<String>, version: impl Into<String>) -> Self {
        self.available_plugins.insert(id.into(), version.into());
        self
    }

    pub fn with_external_provider(
        mut self,
        id: impl Into<String>,
        version: impl Into<String>,
    ) -> Self {
        self.available_external_providers
            .insert(id.into(), version.into());
        self
    }
}

pub fn preflight(job: &BatchJob, options: &PreflightOptions) -> PreflightReport {
    let mut report = PreflightReport::default();
    if let Err(error) = job.validate() {
        report.push(Diagnostic::new(
            DiagnosticSeverity::Error,
            "invalid-job",
            error.to_string(),
        ));
        return report;
    }

    report.push(Diagnostic::new(
        DiagnosticSeverity::Info,
        "checkpoint-policy",
        format!("checkpoint policy is {:?}", job.checkpoint_policy),
    ));
    check_dependencies(job, options, &mut report);
    check_recipes(job, options, &mut report);
    check_items(job, options, &mut report);
    check_disk_space(job, options, &mut report);
    report
}

fn check_dependencies(job: &BatchJob, options: &PreflightOptions, report: &mut PreflightReport) {
    for dependency in &job.dependencies.node_packs {
        let available = options
            .available_node_packs
            .iter()
            .find(|manifest| manifest.package_id == dependency.id);
        match available {
            None => report.push(Diagnostic::new(
                DiagnosticSeverity::Error,
                "missing-node-pack",
                format!(
                    "node pack '{}' version '{}' is not installed",
                    dependency.id, dependency.version
                ),
            )),
            Some(manifest) if manifest.version != dependency.version => {
                report.push(Diagnostic::new(
                    DiagnosticSeverity::Error,
                    "node-pack-version-mismatch",
                    format!(
                        "node pack '{}' requires {}, available {}",
                        dependency.id, dependency.version, manifest.version
                    ),
                ))
            }
            Some(_) => report.push(Diagnostic::new(
                DiagnosticSeverity::Info,
                "node-pack-available",
                format!("node pack '{}' is available", dependency.id),
            )),
        }
    }

    for dependency in &job.dependencies.subgraphs {
        match options.available_subgraphs.get(&dependency.id) {
            None => report.push(Diagnostic::new(
                DiagnosticSeverity::Error,
                "missing-subgraph",
                format!("subgraph '{}' is not installed", dependency.id),
            )),
            Some(hash) if !dependency.hash.is_empty() && hash != &dependency.hash => {
                report.push(Diagnostic::new(
                    DiagnosticSeverity::Error,
                    "subgraph-hash-mismatch",
                    format!("subgraph '{}' has a different content hash", dependency.id),
                ))
            }
            Some(_) => report.push(Diagnostic::new(
                DiagnosticSeverity::Info,
                "subgraph-available",
                format!("subgraph '{}' is available", dependency.id),
            )),
        }
    }

    for (id, required) in &job.dependencies.plugins {
        match options.available_plugins.get(id) {
            None => report.push(Diagnostic::new(
                DiagnosticSeverity::Error,
                "missing-plugin",
                format!("plugin '{id}' version '{required}' is not installed"),
            )),
            Some(available) if available != required => report.push(Diagnostic::new(
                DiagnosticSeverity::Error,
                "plugin-version-mismatch",
                format!("plugin '{id}' requires {required}, available {available}"),
            )),
            Some(_) => report.push(Diagnostic::new(
                DiagnosticSeverity::Info,
                "plugin-available",
                format!("plugin '{id}' is available"),
            )),
        }
    }

    for (id, required) in &job.dependencies.external_providers {
        match options.available_external_providers.get(id) {
            None => report.push(Diagnostic::new(
                DiagnosticSeverity::Warning,
                "unavailable-external-provider",
                format!("external provider '{id}' version '{required}' is unavailable"),
            )),
            Some(available) if available != required => report.push(Diagnostic::new(
                DiagnosticSeverity::Warning,
                "external-provider-version-mismatch",
                format!("external provider '{id}' requires {required}, available {available}"),
            )),
            Some(_) => report.push(Diagnostic::new(
                DiagnosticSeverity::Info,
                "external-provider-available",
                format!("external provider '{id}' is available"),
            )),
        }
    }
}

fn check_recipes(job: &BatchJob, options: &PreflightOptions, report: &mut PreflightReport) {
    let mut planned = BTreeMap::<PathBuf, (usize, String)>::new();
    for (recipe_index, recipe) in job.recipes.iter().enumerate() {
        if let Err(error) = recipe.validate() {
            report.push(Diagnostic::new(
                DiagnosticSeverity::Error,
                "invalid-recipe",
                error.to_string(),
            ));
            continue;
        }
        if options.check_output_directories && !output_directory_is_accessible(&recipe.destination)
        {
            report.push(Diagnostic::new(
                DiagnosticSeverity::Error,
                "output-directory",
                format!(
                    "output directory '{}' cannot be created or written",
                    recipe.destination.display()
                ),
            ));
        }
        for (item_index, item) in job.items.iter().enumerate() {
            let path = rendered_output_path(recipe, item, item_index);
            match planned.insert(path.clone(), (recipe_index, item.id.clone())) {
                Some((previous_index, previous_id))
                    if recipe.collision_policy == CollisionPolicy::Error =>
                {
                    report.push(Diagnostic::for_item(
                        DiagnosticSeverity::Error,
                        "naming-collision",
                        format!(
                            "output '{}' is also produced for item '{}' by recipe {previous_index}",
                            path.display(),
                            previous_id
                        ),
                        &item.id,
                        Some(path.clone()),
                    ));
                }
                _ => {}
            }
            if options.check_collisions
                && path.exists()
                && recipe.collision_policy == CollisionPolicy::Error
            {
                report.push(Diagnostic::for_item(
                    DiagnosticSeverity::Error,
                    "existing-output",
                    format!("output '{}' already exists", path.display()),
                    &item.id,
                    Some(path),
                ));
            } else if options.check_collisions
                && path.exists()
                && recipe.collision_policy == CollisionPolicy::Skip
            {
                report.push(Diagnostic::for_item(
                    DiagnosticSeverity::Warning,
                    "existing-output-skipped",
                    format!("existing output '{}' will be reused", path.display()),
                    &item.id,
                    Some(path),
                ));
            }
        }
    }
}

fn check_items(job: &BatchJob, options: &PreflightOptions, report: &mut PreflightReport) {
    let mut total_bytes = 0_u64;
    for item in &job.items {
        if options.check_source_files && !item.source_path.is_file() {
            report.push(Diagnostic::for_item(
                DiagnosticSeverity::Error,
                "missing-source",
                format!(
                    "source file '{}' does not exist",
                    item.source_path.display()
                ),
                &item.id,
                Some(item.source_path.clone()),
            ));
        } else if let Ok(metadata) = fs::metadata(&item.source_path) {
            total_bytes = total_bytes.saturating_add(metadata.len());
        }
    }
    if job.items.is_empty() {
        report.push(Diagnostic::new(
            DiagnosticSeverity::Warning,
            "empty-job",
            "batch contains no items",
        ));
    }
}

fn check_disk_space(job: &BatchJob, options: &PreflightOptions, report: &mut PreflightReport) {
    let Some(available) = options.available_disk_bytes else {
        return;
    };
    let source_bytes = job
        .items
        .iter()
        .filter_map(|item| fs::metadata(&item.source_path).ok())
        .map(|metadata| metadata.len())
        .sum::<u64>();
    if source_bytes > available {
        report.push(Diagnostic::new(
            DiagnosticSeverity::Error,
            "disk-space-risk",
            format!(
                "available disk space ({available} bytes) is below the estimated input footprint ({source_bytes} bytes)"
            ),
        ));
    } else {
        report.push(Diagnostic::new(
            DiagnosticSeverity::Info,
            "disk-space-check",
            format!("available disk space is {available} bytes"),
        ));
    }
}

fn output_directory_is_accessible(path: &Path) -> bool {
    let mut current = path;
    while !current.exists() {
        let Some(parent) = current.parent() else {
            return false;
        };
        if parent == current {
            return false;
        }
        current = parent;
    }
    if !current.is_dir() {
        return false;
    }
    match fs::metadata(current) {
        Ok(metadata) => !metadata.permissions().readonly(),
        Err(_) => false,
    }
}

#[allow(dead_code)]
fn _unique_paths(job: &BatchJob) -> BTreeSet<PathBuf> {
    job.recipes
        .iter()
        .flat_map(|recipe| {
            job.items
                .iter()
                .enumerate()
                .map(move |(index, item)| rendered_output_path(recipe, item, index))
        })
        .collect()
}
