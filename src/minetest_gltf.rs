use std::path::{Path, PathBuf};

use ahash::AHashMap;
use glam::Mat4;
use gltf::scene::Transform;

use crate::{animation::BoneAnimationChannel, Scene};

///
/// Raw data container to hold GLTF Scene and Animation data.
///
pub struct MinetestGLTF {
  pub scenes: Vec<Scene>,
  // In the future: this will be an AHasMap<String, AHashMap<i32, BoneAnimation>> to support
  // multiple animations by name.
  ///
  /// Access the animation by the node (bone) id.
  ///
  pub bone_animations: AHashMap<i32, BoneAnimationChannel>,
}

impl MinetestGLTF {
  pub fn is_animated(&self) -> bool {
    !self.bone_animations.is_empty()
  }
}

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

pub fn transform_to_matrix(transform: Transform) -> Mat4 {
  let tr = transform.matrix();
  Mat4::from_cols_array(&[
    tr[0][0], tr[0][1], tr[0][2], tr[0][3], tr[1][0], tr[1][1], tr[1][2], tr[1][3], tr[2][0],
    tr[2][1], tr[2][2], tr[2][3], tr[3][0], tr[3][1], tr[3][2], tr[3][3],
  ])
}
