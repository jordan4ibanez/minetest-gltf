use std::path::{Path, PathBuf};

use ahash::AHashMap;

use crate::{animation::BoneAnimationChannel, Model};

// Helps to simplify the signature of import related functions.
///
/// Raw data container to hold GLTF Scene and Animation data.
///
pub struct MinetestGLTF {
  pub(crate) model: Option<Model>,
  // In the future: this will be an AHasMap<String, AHashMap<i32, BoneAnimation>> to support
  // multiple animations by name.
  ///
  /// Access the animation by the node (bone) id.
  ///
  pub bone_animations: AHashMap<i32, BoneAnimationChannel>,

  pub(crate) buffers: Vec<gltf::buffer::Data>,
  pub base_dir: PathBuf,
}

impl MinetestGLTF {
  pub fn new<P>(buffers: Vec<gltf::buffer::Data>, path: P) -> Self
  where
    P: AsRef<Path>,
  {
    let mut base_dir = PathBuf::from(path.as_ref());
    base_dir.pop();
    MinetestGLTF {
      model: None,
      bone_animations: AHashMap::new(),
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
    !self.bone_animations.is_empty()
  }
}
