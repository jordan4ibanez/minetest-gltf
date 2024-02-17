// Based on https://whoisryosuke.com/blog/2022/importing-gltf-with-wgpu-and-rust
// You can thank ryosuke for this information.

/// todo:
pub enum Keyframes {
  /// todo:
  Translation(Vec<Vec<f32>>),
  /// todo:
  Rotation(Vec<Vec<f32>>),
  /// todo:
  Other,
}

/// todo:
pub struct AnimationClip {
  /// todo:
  pub name: String,
  /// todo:
  pub keyframes: Keyframes,
  /// todo:
  pub timestamps: Vec<f32>,
}
