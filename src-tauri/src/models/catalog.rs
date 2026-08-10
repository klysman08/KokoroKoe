use super::ModelDescriptor;

const REVISION: &str = "5359861c739e955e79d9a303bcbc70fb988958b1";
#[cfg(test)]
const REPOSITORY: &str = "https://huggingface.co/ggerganov/whisper.cpp";

pub const TINY: ModelDescriptor = ModelDescriptor {
    id: "whisper-tiny-multilingual",
    engine: "whisper",
    name: "Whisper Tiny (multilingual)",
    source_url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-tiny.bin",
    source_revision: REVISION,
    file_name: "ggml-tiny.bin",
    sha256: "be07e048e1e599ad46341c8d2a135645097a538221678b7acdd1b1919c6e1b21",
    download_bytes: 77_691_713,
    disk_bytes: 77_691_713,
    languages: &["multilingual"],
    approximate_memory_bytes: 512 * 1024 * 1024,
    performance_class: "fast",
    backends: &["cpu", "vulkan"],
    license_spdx: "MIT",
    license_url: "https://huggingface.co/ggerganov/whisper.cpp/blob/5359861c739e955e79d9a303bcbc70fb988958b1/README.md",
};

pub const BASE: ModelDescriptor = ModelDescriptor {
    id: "whisper-base-multilingual",
    engine: "whisper",
    name: "Whisper Base (multilingual)",
    source_url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-base.bin",
    source_revision: REVISION,
    file_name: "ggml-base.bin",
    sha256: "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe",
    download_bytes: 147_951_465,
    disk_bytes: 147_951_465,
    languages: &["multilingual"],
    approximate_memory_bytes: 768 * 1024 * 1024,
    performance_class: "balanced",
    backends: &["cpu", "vulkan"],
    license_spdx: "MIT",
    license_url: "https://huggingface.co/ggerganov/whisper.cpp/blob/5359861c739e955e79d9a303bcbc70fb988958b1/README.md",
};

pub const ALL: &[ModelDescriptor] = &[TINY, BASE];

pub fn find(id: &str) -> Option<&'static ModelDescriptor> {
    ALL.iter().find(|descriptor| descriptor.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_exact_and_immutable() {
        assert_eq!(ALL.len(), 2);
        assert_eq!(TINY.source_revision, REVISION);
        assert_eq!(BASE.source_revision, REVISION);
        for descriptor in ALL {
            assert!(descriptor.source_url.starts_with(REPOSITORY));
            assert!(descriptor.source_url.contains(descriptor.source_revision));
            assert_eq!(descriptor.download_bytes, descriptor.disk_bytes);
            assert_eq!(descriptor.sha256.len(), 64);
            assert_eq!(descriptor.license_spdx, "MIT");
            assert_eq!(descriptor.languages, &["multilingual"]);
            assert_eq!(descriptor.backends, &["cpu", "vulkan"]);
        }
    }
}
