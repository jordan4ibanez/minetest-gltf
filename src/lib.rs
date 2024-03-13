//! This crate is intended to load [glTF 2.0](https://www.khronos.org/gltf), a
//! file format designed for the efficient transmission of 3D assets.
//!
//! It's base on [gltf](https://github.com/gltf-rs/gltf) crate but has an easy to use output.
//!
//! # Installation
//!
//! ```toml
//! [dependencies]
//! easy-gltf="1.1.1"
//! ```
//!
//! # Example
//!
//! ```
//! let mine_gltf = minetest_gltf::load("tests/cube.glb", true).expect("Failed to load glTF");
//! for scene in mine_gltf.scenes {
//!     println!(
//!         "Models: #{}",
//!         scene.models.len()
//!     )
//! }
//! ```

mod minetest_gltf;
mod model;

use gltf::Gltf;
use log::error;
use minetest_gltf::MinetestGLTF;
use model::animation::finalize_animations;
use std::error::Error;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

pub use model::*;

/// Load scenes from path to a glTF 2.0.
///
/// You can choose to enable material loading.
///
/// Note: You can use this function with either a `Gltf` (standard `glTF`) or `Glb` (binary glTF).
///
/// # Example
///
/// ```
/// let minetest_gltf = minetest_gltf::load("tests/cube.glb").expect("Failed to load glTF");
/// let model = &minetest_gltf.model.unwrap(); // Retrieve the first and only model.
/// println!("Primitives: #{}", model.primitives.len());
/// ```
pub fn load(path: &str) -> Result<MinetestGLTF, Box<dyn Error + Send + Sync>> {
  // Run gltf

  // Try to get the file name. If this fails, the path probably doesn't exist.
  let file_name = file_name_from_path(path)?;

  // We need the base path for the GLTF lib. We want to choose if we load textures.
  let base = Path::new(path).parent().unwrap_or_else(|| Path::new("./"));

  // The buffer we're going to read the model into.
  let model_reader = read_path_to_buf_read(path)?;

  // Now we need to get the "Document" from the GLTF lib.
  let gltf_data = Gltf::from_reader(model_reader)?;

  // We always want the buffer data. We have to clone this, it's basically ripping out ownership from our hands.
  let buffers = gltf::import_buffers(&gltf_data.clone(), Some(base), gltf_data.blob.clone())?;

  // Init data and collection useful for conversion
  let mut minetest_gltf = MinetestGLTF::new(buffers.clone(), path);

  // Convert gltf -> minetest_gltf
  let scene_attempt = gltf_data.scenes().next();
  if scene_attempt.is_none() {
    return Err(format!("Model contains no scenes. {}", file_name).into());
  }
  let scene = if let Some(scene) = scene_attempt {
    scene
  } else {
    panic!("blew up after check somehow.")
  };

  let model = Model::load(scene, &mut minetest_gltf);

  // Double check that this model actually exists.
  if model.primitives.first().is_none() {
    return Err("Model has no primitives!".into());
  }

  // Check if the model is able to be animated.
  let mut is_skinned = false;
  for primitive in &model.primitives {
    if primitive.has_joints && primitive.has_weights {
      is_skinned = true;
    } else {
      is_skinned = false;
      break;
    }
  }

  if false {
    if !is_skinned {
      error!("Animation failure on model {}. >:(", file_name);
    } else {
      error!("Model {} is animated. :)", file_name);
    }
  }

  // Now apply the data.
  if is_skinned {
    // If there's an error parsing, raw return the error.
    finalize_animations(&mut minetest_gltf, gltf_data, buffers, file_name)?
  } else {
    minetest_gltf.is_animated = false;
  }

  minetest_gltf.model = Some(model);

  // Now remove temp data.
  minetest_gltf.buffers.clear();

  Ok(minetest_gltf)
}

///
/// Automatically parse a file path into a BufReader<File>.
///
fn read_path_to_buf_read(path: &str) -> Result<BufReader<File>, String> {
  match File::open(path) {
    Ok(file) => Ok(BufReader::new(file)),
    Err(e) => Err(format!("Path to BufReader failure. {}", e)),
  }
}

///
/// Get a file name from the path provided.
///
fn file_name_from_path(path: &str) -> Result<&str, &str> {
  let new_path = Path::new(path);

  if !new_path.exists() {
    return Err("File name from file path. Path does not exist.");
  }

  match new_path.file_name() {
    Some(os_str) => match os_str.to_str() {
      Some(final_str) => Ok(final_str),
      None => Err("File name from file path. Failed to convert OsStr to str."),
    },
    None => Err("File name from file path. Failed to parse OS Path str."),
  }
}

// ? ////////////////////////////////////////////////////////////////////////////////////////////// ? //
// ?                            CODE ENDS HERE, BEGIN UNIT TESTS.                                   ? //
// ? ////////////////////////////////////////////////////////////////////////////////////////////// ? //

#[cfg(test)]
mod tests {
  use crate::*;

  // #[test]
  // fn check_cube_glb() {
  //   drop(env_logger::try_init());

  //   let mine_gltf = match load("tests/cube.glb") {
  //     Ok(mine_gltf) => {
  //       println!("Cube loaded!");
  //       mine_gltf
  //     }
  //     Err(e) => panic!("Cube: failed to load. {}", e),
  //   };
  //   match mine_gltf.model {
  //     Some(model) => {
  //       assert_eq!(model.primitives.len(), 1);
  //     }
  //     None => panic!("cube_glb exploded into pieces. :("),
  //   }
  // }

  // #[test]
  // fn check_different_meshes() {
  //   drop(env_logger::try_init());

  //   let mine_gltf = match load("tests/complete.glb") {
  //     Ok(mine_gltf) => {
  //       println!("Complete loaded!");
  //       mine_gltf
  //     }
  //     Err(e) => panic!("Complete: failed to load. {}", e),
  //   };

  //   match mine_gltf.model {
  //     Some(model) => {
  //       for model in model.primitives {
  //         match model.mode() {
  //           Mode::Triangles | Mode::TriangleFan | Mode::TriangleStrip => {
  //             assert!(model.triangles().is_ok());
  //           }
  //           Mode::Lines | Mode::LineLoop | Mode::LineStrip => {
  //             assert!(model.lines().is_ok());
  //           }
  //           Mode::Points => {
  //             assert!(model.points().is_ok());
  //           }
  //         }
  //       }
  //     }
  //     None => panic!("complete has no model!"),
  //   }
  // }

  // #[test]
  // fn check_cube_gltf() {
  //   drop(env_logger::try_init());

  //   let _ = match load("tests/cube_classic.gltf") {
  //     Ok(mine_gltf) => {
  //       println!("cube_classic loaded!");
  //       mine_gltf
  //     }
  //     Err(e) => panic!("cube_classic: failed to load. {}", e),
  //   };
  // }

  // #[test]
  // fn check_model() {
  //   drop(env_logger::try_init());

  //   let mine_gltf = match load("tests/cube.glb") {
  //     Ok(mine_gltf) => {
  //       println!("cube loaded!");
  //       mine_gltf
  //     }
  //     Err(e) => panic!("cube: failed to load. {}", e),
  //   };
  //   let primitive = match &mine_gltf.model {
  //     Some(model) => match model.primitives.first() {
  //       Some(primitive) => primitive,
  //       None => panic!("cube.glb has no primitives."),
  //     },
  //     None => panic!("cube.glb has no model."),
  //   };
  //   assert!(primitive.has_normals());
  //   assert!(primitive.has_tex_coords());
  //   assert!(primitive.has_tangents());
  //   for t in match primitive.triangles() {
  //     Ok(tris) => tris,
  //     Err(e) => panic!("Failed to get cube tris. {}", e),
  //   }
  //   .iter()
  //   .flatten()
  //   {
  //     let pos = t.position;
  //     assert!(pos.x > -0.01 && pos.x < 1.01);
  //     assert!(pos.y > -0.01 && pos.y < 1.01);
  //     assert!(pos.z > -0.01 && pos.z < 1.01);

  //     // Check that the tangent w component is 1 or -1
  //     assert_eq!(t.tangent.w.abs(), 1.);
  //   }
  // }

  // #[test]
  // fn check_invalid_path() {
  //   drop(env_logger::try_init());

  //   assert!(load("tests/invalid.glb").is_err());
  // }

  // #[test]
  // fn load_snowman() {
  //   drop(env_logger::try_init());

  //   let snowman = match load("tests/snowman.gltf") {
  //     Ok(mine_gltf) => {
  //       println!("Snowman loaded!");
  //       mine_gltf
  //     }
  //     Err(e) => panic!("Snowman: failed to load. {}", e),
  //   };

  //   match snowman.model {
  //     Some(model) => {
  //       assert_eq!(model.primitives.len(), 5);
  //     }
  //     None => panic!("Snowman: has no model."),
  //   }

  //   assert!(!snowman.is_animated);
  // }

  #[test]
  fn test_the_spider_animations() {
    drop(env_logger::try_init());

    let spider = match load("tests/spider_animated.gltf") {
      Ok(mine_gltf) => {
        println!("spider loaded!");
        mine_gltf
      }
      Err(e) => panic!("spider: failed to load. {}", e),
    };

    let animations = match spider.bone_animations {
      Some(animations) => animations,
      None => panic!("spider has no bone animations!"),
    };

    for (_, animation) in animations {
      error!("spider: {}", animation.translation_timestamps.len());
      assert!(animation.translation_timestamps.len() == 121);
      assert!(animation.translations.len() == 121);
      assert!(animation.rotation_timestamps.len() == 121);
      assert!(animation.rotations.len() == 121);
      assert!(animation.scale_timestamps.len() == 121);
      assert!(animation.scales.len() == 121);
    }
  }

  // #[test]
  // fn test_sam() {
  //   drop(env_logger::try_init());

  //   let sam = match load("tests/minetest_sam.gltf") {
  //     Ok(mine_gltf) => {
  //       println!("sam loaded!");
  //       mine_gltf
  //     }
  //     Err(e) => panic!("minetest_sam: failed to load. {}", e),
  //   };

  //   assert!(sam.bone_animations.is_some());

  //   let animations = match sam.bone_animations {
  //     Some(animations) => animations,
  //     None => panic!("sam has no bone animations!"),
  //   };

  //   // println!("sam animations: {},", animations.len());

  //   for (_, animation) in animations {
  //     assert!(animation.translation_timestamps.len() == 221);
  //     assert!(animation.translations.len() == 221);
  //     assert!(animation.rotation_timestamps.len() == 221);
  //     assert!(animation.rotations.len() == 221);
  //     assert!(animation.scale_timestamps.len() == 221);
  //     assert!(animation.scales.len() == 221);
  //   }

  //   match sam.model {
  //     Some(model) => {
  //       assert!(model.primitives.len() == 1);
  //       for primitive in model.primitives {
  //         assert!(primitive.has_joints);
  //         assert!(primitive.has_weights);
  //         assert!(primitive.has_tex_coords());
  //         assert_eq!(primitive.weights.len(), 168);
  //         assert_eq!(primitive.joints.len(), 168);
  //       }
  //     }
  //     None => panic!("sam has no model?!"),
  //   }
  // }

  // #[test]
  // fn load_simple_skin() {
  //   drop(env_logger::try_init());

  //   let simple_skin = match load("tests/simple_skin.gltf") {
  //     Ok(simple_skin) => {
  //       println!("simple_skin loaded!");
  //       simple_skin
  //     }
  //     Err(e) => panic!("simple_skin: failed to load. {}", e),
  //   };

  //   match simple_skin.model {
  //     Some(model) => {
  //       assert!(model.primitives.len() == 1);
  //       for primitive in model.primitives {
  //         assert!(primitive.has_joints);
  //         assert!(primitive.has_weights);
  //         assert_eq!(primitive.weights.len(), 10);
  //         assert_eq!(primitive.joints.len(), 10);
  //       }
  //     }
  //     None => panic!("simple_skin has no model?!"),
  //   }

  //   // This one's a curve ball. This is an ultra simple model so let's see if tries to iterate more than one channel!
  //   match simple_skin.bone_animations {
  //     Some(bone_animations) => {
  //       for (_, channel) in bone_animations {
  //         assert!(
  //           channel.translation_timestamps.len() == channel.translations.len()
  //             && channel.translations.len() == 12
  //         );

  //         assert!(
  //           channel.rotation_timestamps.len() == channel.rotations.len()
  //             && channel.rotations.len() == 12
  //         );

  //         assert!(
  //           channel.scale_timestamps.len() == channel.scales.len() && channel.scales.len() == 12
  //         );

  //         assert!(
  //           channel.weight_timestamps.len() == channel.weights.len() && channel.weights.is_empty()
  //         );
  //       }
  //     }
  //     None => todo!(),
  //   }
  // }
}
