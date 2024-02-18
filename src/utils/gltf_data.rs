use crate::Material;
use ahash::AHashMap;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use gltf::image::Source;
use image::*;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Helps to simplify the signature of import related functions.
pub struct GltfData {
  pub buffers: Vec<gltf::buffer::Data>,
  pub images: Option<Vec<gltf::image::Data>>,
  pub base_dir: PathBuf,
  pub materials: AHashMap<Option<usize>, Arc<Material>>,
  pub rgb_images: AHashMap<usize, Arc<RgbImage>>,
  pub rgba_images: AHashMap<usize, Arc<RgbaImage>>,
  pub gray_images: AHashMap<(usize, usize), Arc<GrayImage>>,
}

impl GltfData {
  pub fn new<P>(
    buffers: Vec<gltf::buffer::Data>,
    images: Option<Vec<gltf::image::Data>>,
    path: P,
  ) -> Self
  where
    P: AsRef<Path>,
  {
    let mut base_dir = PathBuf::from(path.as_ref());
    base_dir.pop();
    GltfData {
      buffers,
      images,
      base_dir,
      materials: Default::default(),
      rgb_images: Default::default(),
      rgba_images: Default::default(),
      gray_images: Default::default(),
    }
  }

  pub fn load_rgb_image(&mut self, texture: &gltf::Texture<'_>) -> Arc<RgbImage> {
    if let Some(image) = self.rgb_images.get(&texture.index()) {
      return image.clone();
    }

    let img = Arc::new(self.load_texture(texture).to_rgb8());
    self.rgb_images.insert(texture.index(), img.clone());
    img
  }

  pub fn load_base_color_image(&mut self, texture: &gltf::Texture<'_>) -> Arc<RgbaImage> {
    if let Some(image) = self.rgba_images.get(&texture.index()) {
      return image.clone();
    }
    let img = Arc::new(self.load_texture(texture).to_rgba8());
    self.rgba_images.insert(texture.index(), img.clone());
    img
  }

  pub fn load_gray_image(&mut self, texture: &gltf::Texture<'_>, channel: usize) -> Arc<GrayImage> {
    if let Some(image) = self.gray_images.get(&(texture.index(), channel)) {
      return image.clone();
    }
    let img = self.load_texture(texture).to_rgba8();
    let mut extract_img = GrayImage::new(img.width(), img.height());
    for (x, y, px) in img.enumerate_pixels() {
      extract_img[(x, y)][0] = px[channel];
    }
    let img = Arc::new(extract_img);
    self
      .gray_images
      .insert((texture.index(), channel), img.clone());
    img
  }

  pub fn load_texture(&self, texture: &gltf::Texture<'_>) -> DynamicImage {
    let g_img = texture.source();
    let buffers = &self.buffers;
    match g_img.source() {
      Source::View { view, mime_type } => {
        let parent_buffer_data = &buffers[view.buffer().index()].0;
        let data = &parent_buffer_data[view.offset()..view.offset() + view.length()];
        let mime_type = mime_type.replace('/', ".");
        match image::load_from_memory_with_format(
          data,
          match ImageFormat::from_path(mime_type.clone()) {
            Ok(format) => format,
            Err(e) => panic!(
              "GltfData: Failed to get image format from image [{}], {}",
              mime_type.clone(),
              e
            ),
          },
        ) {
          Ok(dynamic_image) => dynamic_image,
          Err(e) => panic!("GltfData: Failed to load image [{}]. {}", mime_type, e),
        }
      }
      Source::Uri { uri, mime_type } => {
        if uri.starts_with("data:") {
          let encoded = match uri.split(',').nth(1) {
            Some(data) => data,
            None => panic!("GltfData: Failed to retrieve URI data."),
          };
          let data = match URL_SAFE_NO_PAD.decode(encoded) {
            Ok(data) => data,
            Err(e) => panic!("GltfData: Failed to decode data. {}", e),
          };

          let mime_type = if let Some(ty) = mime_type {
            ty
          } else {
            match uri.split(',').next() {
              Some(raw_1) => match raw_1.split(':').nth(1) {
                Some(raw_2) => match raw_2.split(';').next() {
                  Some(final_mime_type) => final_mime_type,
                  None => panic!("GltfData: Failed to split mime type by semicolon. [raw_2]"),
                },
                None => panic!("GltfData: Failed to split mime type by colon. [raw_1]"),
              },
              None => panic!("GltfData: Failed to split mime type by comma. [uri]"),
            }
          };
          let mime_type = mime_type.replace('/', ".");

          match image::load_from_memory_with_format(
            &data,
            match ImageFormat::from_path(mime_type) {
              Ok(format) => format,
              Err(e) => panic!("GltfData: Failed to get image format from path. {}", e),
            },
          ) {
            Ok(dynamic_image) => dynamic_image,
            Err(e) => panic!(
              "GltfData: Failed to load image from memory with format. {}",
              e
            ),
          }
        } else {
          let path = self.base_dir.join(uri);
          match open(path) {
            Ok(dynamic_image) => dynamic_image,
            Err(e) => panic!(
              "GltfData: Failed to open the image at the specified path. {}",
              e
            ),
          }
        }
      }
    }
  }
}
