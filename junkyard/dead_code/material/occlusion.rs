use crate::utils::GltfData;
use image::GrayImage;
use std::sync::Arc;

#[derive(Clone, Debug)]
/// Defines the occlusion texture of a material.
pub struct Occlusion {
  /// The `occlusion_texture` refers to a texture that defines areas of the
  /// surface that are occluded from light, and thus rendered darker.
  pub texture: Arc<GrayImage>,

  /// The `occlusion_factor` is the occlusion strength to be applied to the
  /// texture value.
  pub factor: f32,
}

impl Occlusion {
  ///
  /// Load up an occlusion texture.
  ///
  pub(crate) fn load(gltf_mat: &gltf::Material, data: &mut GltfData) -> Option<Self> {
    gltf_mat.occlusion_texture().map(|texture| Self {
      texture: data.load_gray_image(&texture.texture(), 0),
      factor: texture.strength(),
    })
  }
}
