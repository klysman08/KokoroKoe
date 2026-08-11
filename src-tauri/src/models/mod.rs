mod catalog;
mod installer;
mod service;

use std::path::PathBuf;

pub(crate) use service::ModelService;

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct VerifiedModelArtifact {
    pub(crate) model_id: String,
    pub(crate) path: PathBuf,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModelDescriptor {
    pub id: &'static str,
    pub engine: &'static str,
    pub name: &'static str,
    pub source_url: &'static str,
    pub source_revision: &'static str,
    pub file_name: &'static str,
    pub sha256: &'static str,
    pub download_bytes: u64,
    pub disk_bytes: u64,
    pub languages: &'static [&'static str],
    pub approximate_memory_bytes: u64,
    pub performance_class: &'static str,
    pub backends: &'static [&'static str],
    pub license_spdx: &'static str,
    pub license_url: &'static str,
}
