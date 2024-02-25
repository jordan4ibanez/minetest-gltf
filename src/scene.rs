/// Contains animation data for the models.
pub mod animation;
/// Contains model and material
/// # Usage
/// Check [Model](struct.Model.html) for more information about how to use this module.
pub mod model;

use crate::utils::transform_to_matrix;
use crate::GltfData;
use glam::Mat4;
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

    for (index, node) in gltf_scene.nodes().enumerate() {
      scene.read_node(&node, &Mat4::IDENTITY, data, load_materials);
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
