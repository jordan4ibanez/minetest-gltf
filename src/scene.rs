/// Contains animation data for the models.
pub mod animation;
mod camera;
mod light;
/// Contains model and material
/// # Usage
/// Check [Model](struct.Model.html) for more information about how to use this module.
pub mod model;

use crate::utils::transform_to_matrix;
use crate::GltfData;
pub use camera::{Camera, Projection};
use glam::Mat4;
pub use light::Light;
use log::error;
pub use model::{Material, Model};

use gltf::scene::Node;

/// Contains cameras, models and lights of a scene.
#[derive(Default, Clone, Debug)]
pub struct Scene {
  #[cfg(feature = "names")]
  /// Scene name. Requires the `names` feature.
  pub name: Option<String>,
  #[cfg(feature = "extras")]
  /// Scene extra data. Requires the `extras` feature.
  pub extras: gltf::json::extras::Extras,
  /// List of models in the scene.
  pub models: Vec<Model>,
  /// List of cameras in the scene.
  pub cameras: Vec<Camera>,
  /// List of lights in the scene.
  pub lights: Vec<Light>,
  /// List of weights in the scene.
  pub weights: Option<Vec<f32>>,
}

impl Scene {
  pub(crate) fn load(gltf_scene: gltf::Scene, data: &mut GltfData, load_materials: bool) -> Self {
    let mut scene = Self::default();

    #[cfg(feature = "names")]
    {
      scene.name = gltf_scene.name().map(String::from);
    }
    #[cfg(feature = "extras")]
    {
      scene.extras = gltf_scene.extras().clone();
    }

    println!("this has {} nodes", gltf_scene.nodes().len());

    if let Some(root_node) = gltf_scene.nodes().next() {

      // if let Some(mesh) = root_node.mesh() {
      //   for primitive in mesh.primitives() {
      //     let all_attributes = primitive.attributes();

      //     for (semantic, attribute) in all_attributes {
      //       println!("{:?}", semantic);
      //     }
      //   }
      // } else {
      //   error!("no mesh");
      // }
    } else {
      error!("no root node");
    }

    for (index, node) in gltf_scene.nodes().enumerate() {
      scene.read_node(&node, &Mat4::IDENTITY, data, load_materials);

      println!("index: {}", index);

      if let Some(skin) = node.skin() {
      } else {
        error!("no skin index {}", index);
      }

      if let Some(mesh) = node.mesh() {
      } else {
        error!("no mesh index {}", index);
      }

      // Try to load weights and joints.
      // if let Some(skin) = node.skin() {
      //   for joint in skin.joints() {
      //     println!("joint: {}", joint.index());
      //     for child in joint.children() {
      //       println!("child: {}", child.index())
      //     }
      //   }
      // } else {
      //   error!("this doesn't have a skin!");
      // }
    }
    scene
  }

  fn read_node(
    &mut self,
    node: &Node,
    parent_transform: &Mat4,
    data: &mut GltfData,
    load_materials: bool,
  ) {
    // Compute transform of the current node.
    let transform = *parent_transform * transform_to_matrix(node.transform());

    // Recurse on children.
    for child in node.children() {
      self.read_node(&child, &transform, data, load_materials);
    }

    // Load camera.
    if let Some(camera) = node.camera() {
      self.cameras.push(Camera::load(camera, &transform));
    }

    // Load light.
    if let Some(light) = node.light() {
      self.lights.push(Light::load(light, &transform));
    }

    // Try to load weights.
    if node.weights().is_none() {
      error!("oh no");
    }
    self.weights = node.weights().map(|weights| weights.to_vec());

    // Load model
    if let Some(mesh) = node.mesh() {
      for (i, primitive) in mesh.primitives().enumerate() {
        self.models.push(Model::load(
          &mesh,
          i,
          primitive,
          &transform,
          data,
          load_materials,
        ));
      }
    }
  }
}
