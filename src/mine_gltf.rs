use crate::{animation::AnimationClip, Scene};

///
/// Raw data container to hold GLTF Scene and Animation data.
///
pub struct MineGLTF {
  pub scenes: Vec<Scene>,
  pub animations: Vec<AnimationClip>,
}
