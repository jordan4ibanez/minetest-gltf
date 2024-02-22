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
  /// Translation timestamp data.
  pub translation_timestamps: Vec<f32>,

  /// Rotation data.
  pub rotations: Vec<Quat>,
  /// Rotation timestamp data.
  pub rotation_timestamps: Vec<f32>,

  /// Scale data.
  pub scales: Vec<Vec3>,
  /// Scale timestamp data.
  pub scale_timestamps: Vec<f32>,

  /// Weight data.
  pub weights: Vec<f32>,
  /// Not sure why you'll need this but it's here.
  ///
  /// Weight timestamp data.
  pub weights_timestamps: Vec<f32>,
}

impl BoneAnimationChannel {
  ///
  /// Create new bone animation.
  ///
  pub fn new() -> Self {
    BoneAnimationChannel {
      translations: vec![],
      translation_timestamps: vec![],
      rotations: vec![],
      rotation_timestamps: vec![],
      scales: vec![],
      scale_timestamps: vec![],
      weights: vec![],
      weights_timestamps: vec![],
    }
  }
}
