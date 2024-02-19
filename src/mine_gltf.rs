use ahash::AHashMap;

use crate::{animation::BoneAnimationChannel, Scene};

///
/// Raw data container to hold GLTF Scene and Animation data.
///
pub struct MineGLTF {
  pub scenes: Vec<Scene>,
  // In the future: this will be an AHasMap<String, AHashMap<i32, BoneAnimation>> to support
  // multiple animations by name.
  ///
  /// Access the animation by the node (bone) id.
  ///
  pub bone_animations: AHashMap<i32, BoneAnimationChannel>,
}

impl MineGLTF {
  pub fn is_animated(&self) -> bool {
    !self.bone_animations.is_empty()
  }
}
