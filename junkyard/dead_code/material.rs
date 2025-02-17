/// Module for loading up emissive materials. (lit material)
mod emissive;
/// Module for loading up normal maps. (fake bumps and dents)
mod normal;
/// Module for loading up occlusion textures. (light distribution)
mod occlusion;
/// Module for loading up pbr materials. (metallic roughness)
mod pbr;

use crate::utils::*;
use core::ops::Deref;
use glam::{Vec2, Vec3, Vec4};
use image::{ImageBuffer, Pixel};
use std::{ops::Mul, sync::Arc};

pub use emissive::Emissive;
pub use normal::NormalMap;
pub use occlusion::Occlusion;
pub use pbr::PbrMaterial;

/// Contains material properties of models.
#[derive(Clone, Debug, Default)]
pub struct Material {
  #[cfg(feature = "names")]
  /// Material name. Requires the `names` feature.
  pub name: Option<String>,

  #[cfg(feature = "extras")]
  /// Material extra data. Requires the `extras` feature.
  pub extras: gltf::json::extras::Extras,

  /// Parameter values that define the metallic-roughness material model from
  /// Physically-Based Rendering (PBR) methodology.
  pub pbr: PbrMaterial,

  /// Defines the normal texture of a material.
  pub normal: Option<NormalMap>,

  /// Defines the occlusion texture of a material.
  pub occlusion: Option<Occlusion>,

  /// The emissive color of the material.
  pub emissive: Emissive,
}

impl Material {
  /// Get the color base Rgb(A) (in RGB-color space) of the material given a
  /// texture coordinate. If no `base_color_texture` is available then the
  /// `base_color_factor` is returned.
  ///
  /// **Important**: `tex_coords` must contain values between `[0., 1.]`
  /// otherwise the function will fail.
  pub fn get_base_color_alpha(&self, tex_coords: Vec2) -> Vec4 {
    let mut res = self.pbr.base_color_factor;
    if let Some(texture) = &self.pbr.base_color_texture {
      let px_u = Self::get_pixel(tex_coords, texture);
      // Transform to float
      let mut px_f = Vec4::new(0., 0., 0., 0.);
      for i in 0..4 {
        px_f[i] = (px_u[i] as f32) / 255.;
      }
      // Convert sRGB to RGB
      let pixel = Vec4::new(px_f.x.powf(2.2), px_f.y.powf(2.2), px_f.z.powf(2.2), px_f.w);
      // Multiply to the scale factor
      for i in 0..4 {
        res[i] *= pixel[i];
      }
    }
    res
  }

  /// Get the color base Rgb (in RGB-color space) of the material given a
  /// texture coordinate. If no `base_color_texture` is available then the
  /// `base_color_factor` is returned.
  ///
  /// **Important**: `tex_coords` must contain values between `[0., 1.]`
  /// otherwise the function will fail.
  pub fn get_base_color(&self, tex_coords: Vec2) -> Vec3 {
    self.get_base_color_alpha(tex_coords).truncate()
  }

  /// Get the metallic value of the material given a texture coordinate. If no
  /// `metallic_texture` is available then the `metallic_factor` is returned.
  ///
  /// **Important**: `tex_coords` must contain values between `[0., 1.]`
  /// otherwise the function will fail.
  pub fn get_metallic(&self, tex_coords: Vec2) -> f32 {
    self.pbr.metallic_factor
      * if let Some(texture) = &self.pbr.metallic_texture {
        Self::get_pixel(tex_coords, texture)[0] as f32 / 255.
      } else {
        1.
      }
  }

  /// Get the roughness value of the material given a texture coordinate. If no
  /// `roughness_texture` is available then the `roughness_factor` is returned.
  ///
  /// **Important**: `tex_coords` must contain values between `[0., 1.]`
  /// otherwise the function will fail.
  pub fn get_roughness(&self, tex_coords: Vec2) -> f32 {
    self.pbr.roughness_factor
      * if let Some(texture) = &self.pbr.roughness_texture {
        Self::get_pixel(tex_coords, texture)[0] as f32 / 255.
      } else {
        1.
      }
  }

  /// Get the normal vector of the material given a texture coordinate. If no
  /// `normal_texture` is available then `None` is returned.
  ///
  /// **Important**: `tex_coords` must contain values between `[0., 1.]`
  /// otherwise the function will fail.
  pub fn get_normal(&self, tex_coords: Vec2) -> Option<Vec3> {
    let normal = self.normal.as_ref()?;
    let pixel = Self::get_pixel(tex_coords, &normal.texture);
    Some(
      normal.factor
        * Vec3::new(
          (pixel[0] as f32) / 127.5 - 1.,
          (pixel[1] as f32) / 127.5 - 1.,
          (pixel[2] as f32) / 127.5 - 1.,
        ),
    )
  }

  /// Get the occlusion value of the material given a texture coordinate. If no
  /// `occlusion_texture` is available then `None` is returned.
  ///
  /// **Important**: `tex_coords` must contain values between `[0., 1.]`
  /// otherwise the function will fail.
  pub fn get_occlusion(&self, tex_coords: Vec2) -> Option<f32> {
    let occlusion = self.occlusion.as_ref()?;
    Some(occlusion.factor * (Self::get_pixel(tex_coords, &occlusion.texture)[0] as f32 / 255.))
  }

  /// Get the emissive color Rgb of the material given a texture coordinate.
  /// If no `emissive_texture` is available then the `emissive_factor` is
  /// returned.
  ///
  /// **Important**: `tex_coords` must contain values between `[0., 1.]`
  /// otherwise the function will fail.
  pub fn get_emissive(&self, tex_coords: Vec2) -> Vec3 {
    let mut res = self.emissive.factor;
    if let Some(texture) = &self.emissive.texture {
      let pixel = Self::get_pixel(tex_coords, texture);
      for i in 0..3 {
        res[i] *= (pixel[i] as f32) / 255.;
      }
    }
    res
  }

  ///
  /// Get a pixel from a material.
  ///
  fn get_pixel<P, Container>(tex_coords: Vec2, texture: &ImageBuffer<P, Container>) -> P
  where
    P: Pixel + 'static,
    P::Subpixel: 'static,
    Container: Deref<Target = [P::Subpixel]>,
  {
    let coords = tex_coords.mul(Vec2::new(texture.width() as f32, texture.height() as f32));

    texture[(
      (coords.x as i64).rem_euclid(texture.width() as i64) as u32,
      (coords.y as i64).rem_euclid(texture.height() as i64) as u32,
    )]
  }

  ///
  /// Load a material into this container for future use.
  ///
  pub(crate) fn load(gltf_mat: gltf::Material, data: &mut GltfData) -> Arc<Self> {
    if let Some(material) = data.materials.get(&gltf_mat.index()) {
      return material.clone();
    }

    let material = Arc::new(Material {
      #[cfg(feature = "names")]
      name: gltf_mat.name().map(String::from),
      #[cfg(feature = "extras")]
      extras: gltf_mat.extras().clone(),

      pbr: PbrMaterial::load(gltf_mat.pbr_metallic_roughness(), data),
      normal: NormalMap::load(&gltf_mat, data),
      occlusion: Occlusion::load(&gltf_mat, data),
      emissive: Emissive::load(&gltf_mat, data),
    });

    // Add to the collection
    data.materials.insert(gltf_mat.index(), material.clone());
    material
  }
}
