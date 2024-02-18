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

/// Container for raw animation data for a node (bone).
pub struct AnimationComponent {
  /// The name of the animation.
  pub name: String,
  /// The raw keyframe data.
  pub keyframes: Keyframes,
  /// The raw keyframe timestamps.
  pub timestamps: Vec<f32>,
}
