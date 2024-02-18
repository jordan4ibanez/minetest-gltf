use ahash::AHashMap;

use crate::{animation::BoneAnimation, Scene};

///
/// Raw data container to hold GLTF Scene and Animation data.
///
pub struct MineGLTF {
  pub scenes: Vec<Scene>,
  // In the future: this will be an AHasMap<String, AHashMap<i32, BoneAnimation>> to support
  // multiple animations by name.
  pub bone_animation: AHashMap<i32, BoneAnimation>,
}
