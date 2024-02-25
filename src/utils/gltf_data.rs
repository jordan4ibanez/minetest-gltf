use ahash::AHashMap;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use gltf::image::Source;
use image::*;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Helps to simplify the signature of import related functions.
pub struct GltfData {
  pub buffers: Vec<gltf::buffer::Data>,
  pub base_dir: PathBuf,
}

impl GltfData {
  pub fn new<P>(buffers: Vec<gltf::buffer::Data>, path: P) -> Self
  where
    P: AsRef<Path>,
  {
    let mut base_dir = PathBuf::from(path.as_ref());
    base_dir.pop();
    GltfData { buffers, base_dir }
  }
}
