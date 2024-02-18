// Based on https://whoisryosuke.com/blog/2022/importing-gltf-with-wgpu-and-rust
// You can thank ryosuke for this information.

use glam::{Quat, Vec3};

/// Container for raw animation data.
pub struct AnimationData {
  /// The name of the animation.
  pub name: String,
  /// Translation data.
  pub translations: Vec<Vec3>,
  /// Rotation data.
  pub rotations: Vec<Quat>,
  /// Scale data.
  pub scales: Vec<Vec3>,
  /// Weight data.
  pub weights: Vec<f32>,
  /// The raw keyframe timestamps.
  pub timestamps: Vec<f32>,
}
