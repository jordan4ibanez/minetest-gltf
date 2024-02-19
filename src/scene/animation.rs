// Based on https://whoisryosuke.com/blog/2022/importing-gltf-with-wgpu-and-rust
// You can thank ryosuke for this information.

use glam::{Quat, Vec3};

/// Raw animation data. Unionized.
pub enum Keyframes {
  /// Translation raw data.
  Translation(Vec<Vec3>),
  /// Rotation raw data.
  Rotation(Vec<Quat>),
  /// Scale raw data.
  Scale(Vec<Vec3>),
  /// Morph Target Weights raw data.
  Weights(Vec<f32>),
}

/// Container containing raw TRS animation data for a node (bone).
pub struct BoneAnimation {
  /// The name of the animation.
  pub name: String,
  /// Translation data.
  pub translations: Vec<Vec3>,
  /// Rotation data.
  pub rotations: Vec<Quat>,
  /// Scale data.
  pub scales: Vec<Vec3>,
  /// Keyframe timestamps.
  pub timestamps: Vec<f32>,
}


