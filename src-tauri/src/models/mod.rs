mod catalog;
mod installer;

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
