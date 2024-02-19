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
#[derive(Default)]
pub struct BoneAnimationChannel {
  /// Translation data.
  pub translations: Vec<Vec3>,
  /// Rotation data.
  pub rotations: Vec<Quat>,
  /// Scale data.
  pub scales: Vec<Vec3>,
  /// Keyframe timestamps.
  pub timestamps: Vec<f32>,
}

impl BoneAnimationChannel {
  ///
  /// Create new bone animation.
  ///
  pub fn new() -> Self {
    BoneAnimationChannel {
      translations: vec![],
      rotations: vec![],
      scales: vec![],
      timestamps: vec![],
    }
  }
}
