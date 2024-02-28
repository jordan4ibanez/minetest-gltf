/// Contains animation data for the models.
pub mod animation;
/// Contains model and material
/// # Usage
/// Check [Model](struct.Model.html) for more information about how to use this module.
pub mod primitive;

use crate::minetest_gltf::MinetestGLTF;
use glam::Mat4;

pub use primitive::Primitive;

use gltf::scene::{Node, Transform};

/// Contains primitives of a model.
#[derive(Default, Clone, Debug)]
pub struct Model {
  #[cfg(feature = "names")]
  /// Scene name. Requires the `names` feature.
  pub name: Option<String>,
  #[cfg(feature = "extras")]
  /// Scene extra data. Requires the `extras` feature.
  pub extras: gltf::json::extras::Extras,
  /// List of models in the scene.
  pub primitives: Vec<Primitive>,
}

impl Model {
  pub(crate) fn load(gltf_scene: gltf::Scene, data: &mut MinetestGLTF) -> Self {
    let mut scene = Self::default();

    #[cfg(feature = "names")]
    {
      scene.name = gltf_scene.name().map(String::from);
    }
    #[cfg(feature = "extras")]
    {
      scene.extras = gltf_scene.extras().clone();
    }

    for node in gltf_scene.nodes() {
      scene.read_node(&node, &Mat4::IDENTITY, data);
    }
    scene
  }

  fn read_node(&mut self, node: &Node, parent_transform: &Mat4, data: &mut MinetestGLTF) {
    // Compute transform of the current node.
    let transform = *parent_transform * transform_to_matrix(node.transform());

    // Recurse on children.
    for child in node.children() {
      self.read_node(&child, &transform, data);
    }

    // Load model
    if let Some(mesh) = node.mesh() {
      for (i, primitive) in mesh.primitives().enumerate() {
        self
          .primitives
          .push(Primitive::load(&mesh, i, primitive, &transform, data));
      }
    }
  }
}

fn transform_to_matrix(transform: Transform) -> Mat4 {
  let tr = transform.matrix();
  Mat4::from_cols_array(&[
    tr[0][0], tr[0][1], tr[0][2], tr[0][3], tr[1][0], tr[1][1], tr[1][2], tr[1][3], tr[2][0],
    tr[2][1], tr[2][2], tr[2][3], tr[3][0], tr[3][1], tr[3][2], tr[3][3],
  ])
}
