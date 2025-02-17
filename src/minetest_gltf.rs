use std::path::{Path, PathBuf};

use ahash::AHashMap;

use crate::{animation::BoneAnimationChannel, Model};

// Helps to simplify the signature of import related functions.
///
/// Raw data container to hold GLTF Scene and Animation data.
///
pub struct MinetestGLTF {
  pub model: Option<Model>,
  // In the future: this will be an AHasMap<String, AHashMap<i32, BoneAnimation>> to support
  // multiple animations by name.
  ///
  /// Access the animation by the node (bone) id.
  ///
  pub bone_animations: Option<AHashMap<i32, BoneAnimationChannel>>,
  pub is_animated: bool,

  pub(crate) buffers: Vec<gltf::buffer::Data>,
  pub base_dir: PathBuf,
}

impl MinetestGLTF {
  pub fn new(buffers: Vec<gltf::buffer::Data>, path: &str) -> Self {
    let mut base_dir = PathBuf::from(Path::new(path));
    base_dir.pop();
    MinetestGLTF {
      model: None,
      bone_animations: None,
      is_animated: false,
      buffers,
      base_dir,
    }
  }

  ///
  /// Get if the model is broken.
  ///
  pub fn is_broken(&self) -> bool {
    self.model.is_none()
  }

  ///
  /// Get if the model is animated.
  ///
  pub fn is_animated(&self) -> bool {
    self.bone_animations.is_some()
  }
}
