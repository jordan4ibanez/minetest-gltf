use crate::{animation::AnimationData, Scene};

///
/// Raw data container to hold GLTF Scene and Animation data.
///
pub struct MineGLTF {
  pub scenes: Vec<Scene>,
  pub animations: Vec<AnimationData>,
}
