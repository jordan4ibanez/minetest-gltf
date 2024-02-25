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
//!         "Cameras: #{}  Lights: #{}  Models: #{}",
//!         scene.cameras.len(),
//!         scene.lights.len(),
//!         scene.models.len()
//!     )
//! }
//! ```

mod mine_gltf;
mod scene;
mod utils;

use ahash::AHashMap;
use glam::{Quat, Vec3};
use gltf::animation::util;
use gltf::Gltf;
use itertools::Itertools;
use log::error;
use mine_gltf::MineGLTF;
use scene::animation::{BoneAnimationChannel, Keyframes};
use std::error::Error;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use utils::GltfData;

pub use scene::*;

///
/// This is an extremely specific macro to raw cast an Array4 into an f32 Array4.
///
/// ! This is for testing.
///
macro_rules! raw_cast_array4 {
  ($x:expr) => {{
    let mut returning_array: [f32; 4] = [0.0, 0.0, 0.0, 0.0];
    for (i, v) in $x.iter().enumerate() {
      returning_array[i] = *v as f32;
    }
    returning_array
  }};
}

///
/// This cleans up the implementation when parsing the GLTF rotation data.
///
/// It converts &[[T; 4]] into a Vec<Quat> which is the Keyframes::Rotation enum.
///
macro_rules! quaternionify {
  ($x:expr) => {
    Keyframes::Rotation(
      $x.map(|rot| Quat::from_array(raw_cast_array4!(rot)))
        .collect(),
    )
  };
}

///
/// This cleans up the implementation when parsing the GLTF morph target weights.
///
/// It converts &[T] into a Vec<f32> which is the Keyframes::Weights enum.
///
macro_rules! weightify {
  ($x:expr) => {{
    let mut container: Vec<f32> = vec![];

    // There can be a bug in the iterator given due to how GLTF works, we want to drop out when the end is hit.
    // This prevents an infinite loop.
    let limit = $x.len();
    for (index, value) in $x.enumerate() {
      container.push(value as f32);
      // Bail out.
      if index >= limit {
        break;
      }
    }
    Keyframes::Weights(container)
  }};
}

/// Load scenes from path to a glTF 2.0.
///
/// You can choose to enable material loading.
///
/// Note: You can use this function with either a `Gltf` (standard `glTF`) or `Glb` (binary glTF).
///
/// # Example
///
/// ```
/// let mine_gltf = minetest_gltf::load("tests/cube.glb", true).expect("Failed to load glTF");
/// println!("Scenes: #{}", mine_gltf.scenes.len()); // Output "Scenes: #1"
/// let scene = &mine_gltf.scenes[0]; // Retrieve the first and only scene
/// println!("Cameras: #{}", scene.cameras.len());
/// println!("Lights: #{}", scene.lights.len());
/// println!("Models: #{}", scene.models.len());
/// ```
pub fn load(path: &str) -> Result<MineGLTF, Box<dyn Error + Send + Sync>> {
  // Run gltf

  // Try to get the file name. If this fails, the path probably doesn't exist.
  let file_name = file_name_from_path(path)?;

  // We need the base path for the GLTF lib. We want to choose if we load textures.
  let base = Path::new(path).parent().unwrap_or_else(|| Path::new("./"));

  // The buffer we're going to read the model into.
  let model_reader = read_path_to_buf_read(path)?;

  // Now we need to get the "Document" from the GLTF lib.
  let gltf_data = Gltf::from_reader(model_reader)?;

  // todo: placeholder currently.
  let animation_todo: Option<i32> = None;
  let mut is_animated = true;

  // Only want the first mesh.
  // ? This can probably be updated to allow complex scenes in the future.
  // ! fixme: Turn this into a match please.
  if let Some(mesh) = gltf_data.meshes().next() {
    if let Some(primitive) = mesh.primitives().next() {
      primitive.attributes().for_each(|(semantic, attribute)| {
        println!("{:?}", attribute);
      });
      for (semantic, attribute) in primitive.attributes() {}
    } else {
      is_animated = false;
    }
  } else {
    // This one, is actually a fatal error.
    error!("Model contains no mesh data. Broken.");
    is_animated = false;
  }

  // todo: placeholder.
  if animation_todo.is_none() {
    println!("{}", is_animated);
    println!("Model is not animated.");
  }

  // We're going to do some manual integration here.

  // We always want the buffer data. We have to clone this, it's basically ripping out ownership from our hands.
  let buffers = gltf::import_buffers(&gltf_data.clone(), Some(base), gltf_data.blob.clone())?;

  // Init data and collection useful for conversion
  let mut data = GltfData::new(buffers.clone(), path);

  // Convert gltf -> minetest_gltf
  // ! THIS SHOULD ONLY DO THE FIRST SCENE !
  let mut scenes = vec![];
  for scene in gltf_data.scenes() {
    scenes.push(Scene::load(scene, &mut data));
  }

  // We always want the animation data as well.
  // You can thank: https://whoisryosuke.com/blog/2022/importing-gltf-with-wgpu-and-rust
  let mut bone_animation_channels: AHashMap<i32, BoneAnimationChannel> = AHashMap::new();

  // ? We are mimicking minetest C++ and only getting the first animation.
  if let Some(first_animation) = gltf_data.animations().next() {
    // ? Now we want to get all channels which contains node (bone) TRS data in random order.
    for (channel_index, channel) in first_animation.channels().enumerate() {
      let reader = channel.reader(|buffer| Some(&buffers[buffer.index()]));

      // * If the timestamp accessor is sparse, or something has gone horribly wrong, it's a static model.
      let result_timestamps = if let Some(inputs) = reader.read_inputs() {
        match inputs {
          gltf::accessor::Iter::Standard(times) => {
            let times: Vec<f32> = times.collect();
            // println!("Time: {}", times.len());
            // dbg!(times);
            Ok(times)
          }
          gltf::accessor::Iter::Sparse(_) => Err(format!(
            "minetest-gltf: Sparse keyframes not supported. Model: [{}]. Model will not be animated.",
            file_name
          )),
        }
      } else {
        Err(format!("minetest-gltf: No animation data detected in animation channel [{}]. [{}] is probably a broken model. Model will not be animated.", channel_index, file_name))
      };

      // * If something blows up when parsing the model animations, it's now a static model.
      match result_timestamps {
        Ok(timestamps) => {
          let keyframes = if let Some(outputs) = reader.read_outputs() {
            match outputs {
              util::ReadOutputs::Translations(translation) => {
                Keyframes::Translation(translation.map(Vec3::from_array).collect())
              }
              util::ReadOutputs::Rotations(rotation) => match rotation {
                util::Rotations::I8(rotation) => quaternionify!(rotation),
                util::Rotations::U8(rotation) => quaternionify!(rotation),
                util::Rotations::I16(rotation) => quaternionify!(rotation),
                util::Rotations::U16(rotation) => quaternionify!(rotation),
                util::Rotations::F32(rotation) => quaternionify!(rotation),
              },
              util::ReadOutputs::Scales(scale) => {
                Keyframes::Scale(scale.map(Vec3::from_array).collect())
              }
              util::ReadOutputs::MorphTargetWeights(target_weight) => match target_weight {
                util::MorphTargetWeights::I8(weights) => weightify!(weights),
                util::MorphTargetWeights::U8(weights) => weightify!(weights),
                util::MorphTargetWeights::I16(weights) => weightify!(weights),
                util::MorphTargetWeights::U16(weights) => weightify!(weights),
                util::MorphTargetWeights::F32(weights) => weightify!(weights),
              },
            }
          } else {
            // * Something blew up, it's now a static model.
            error!(
              "minetest-gltf: Unknown keyframe in model [{}]. This model is probably corrupted. Model will not be animated.",
              file_name
            );
            bone_animation_channels.clear();
            break;
          };

          let bone_id = channel.target().node().index() as i32;

          match keyframes {
            Keyframes::Translation(translations) => {
              let animation_channel = bone_animation_channels.entry(bone_id).or_default();

              // * If the animation already has translation for this node (bone), that means that something has gone horribly wrong.
              if !animation_channel.translations.is_empty() {
                error!("minetest-gltf: Attempted to overwrite node (bone) channel [{}]'s translation animation data! Model [{}] is broken! This is now a static model.", bone_id, file_name);
                bone_animation_channels.clear();
                break;
              }

              // * If the translation animation channel data does not match the length of timestamp data, it blew up.
              if translations.len() != timestamps.len() {
                error!(
                  "minetest-gltf: Mismatched node (bone) translations length in channel [{}] of model [{}]. [{}] translation compared to [{}] timestamps. This is now a static model.", 
                  bone_id,
                  file_name,
                  translations.len(),
                  timestamps.len());

                bone_animation_channels.clear();
                break;
              }

              animation_channel.translations = translations;
              animation_channel.translation_timestamps = timestamps;
            }

            Keyframes::Rotation(rotations) => {
              let animation_channel = bone_animation_channels.entry(bone_id).or_default();

              // * If the animation already has rotation for this node (bone), that means that something has gone horribly wrong.
              if !animation_channel.rotations.is_empty() {
                error!("minetest-gltf: Attempted to overwrite node (bone) channel [{}]'s rotation animation data! Model [{}] is broken! This is now a static model.", bone_id, file_name);
                bone_animation_channels.clear();
                break;
              }

              // * If the rotations animation channel data does not match the length of timestamp data, it blew up.
              if rotations.len() != timestamps.len() {
                error!(
                  "minetest-gltf: Mismatched node (bone) rotations length in channel [{}] of model [{}]. [{}] rotation compared to [{}] timestamps. This is now a static model.", 
                  bone_id,
                  file_name,
                  rotations.len(),
                  timestamps.len());

                bone_animation_channels.clear();
                break;
              }

              animation_channel.rotations = rotations;
              animation_channel.rotation_timestamps = timestamps;
            }
            Keyframes::Scale(scales) => {
              let gotten_animation_channel = bone_animation_channels.entry(bone_id).or_default();

              // * If the animation already has scale for this node (bone), that means that something has gone horribly wrong.
              if !gotten_animation_channel.scales.is_empty() {
                error!("minetest-gltf: Attempted to overwrite node (bone) channel [{}]'s scale animation data! Model [{}] is broken! This is now a static model", bone_id, file_name);
                bone_animation_channels.clear();
                break;
              }

              // * If the scales animation channel data does not match the length of timestamp data, it blew up.
              if scales.len() != timestamps.len() {
                error!(
                  "minetest-gltf: Mismatched node (bone) scales length in channel [{}] of model [{}]. [{}] scale compared to [{}] timestamps. This is now a static model.", 
                  bone_id,
                  file_name,
                  scales.len(),
                  timestamps.len());

                bone_animation_channels.clear();
                break;
              }

              gotten_animation_channel.scales = scales;
              gotten_animation_channel.scale_timestamps = timestamps;
            }
            Keyframes::Weights(weights) => {
              let gotten_animation_channel = bone_animation_channels.entry(bone_id).or_default();

              // * If the animation already has weight for this node (bone), that means that something has gone horribly wrong.
              if !gotten_animation_channel.weights.is_empty() {
                error!("minetest-gltf: Attempted to overwrite node (bone) channel [{}]'s weight animation data! Model [{}] is broken! This is now a static model", bone_id, file_name);
                bone_animation_channels.clear();
                break;
              }

              // ? We don't do a timestamp comparison here because weights probably shouldn't have timestamp data anyways??

              gotten_animation_channel.weights = weights;
              gotten_animation_channel.weight_timestamps = timestamps;
            }
          }
        }

        // * Something blew up, it's now a static model.
        Err(e) => {
          error!("{}", e);
          bone_animation_channels.clear();
          break;
        }
      }
    }
  }

  //MC9512H126V
  Ok(MineGLTF {
    scenes,
    bone_animations: bone_animation_channels,
  })
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

#[cfg(test)]
mod tests {
  use crate::model::Mode;
  use crate::*;
  use glam::Vec3;

  macro_rules! assert_delta {
    ($x:expr, $y:expr, $d:expr) => {
      if !($x - $y < $d || $y - $x < $d) {
        panic!();
      }
    };
  }

  // #[test]
  // fn check_cube_glb() {
  //   drop(env_logger::try_init());

  //   let mine_gltf = match load("tests/cube.glb", true) {
  //     Ok(mine_gltf) => {
  //       println!("Cube loaded!");
  //       mine_gltf
  //     }
  //     Err(e) => panic!("Cube: failed to load. {}", e),
  //   };

  //   assert_eq!(mine_gltf.scenes.len(), 1);
  //   let scene = &mine_gltf.scenes[0];
  //   assert_eq!(scene.cameras.len(), 1);
  //   assert_eq!(scene.lights.len(), 3);
  //   assert_eq!(scene.models.len(), 1);
  // }

  // #[test]
  // fn check_different_meshes() {
  //   drop(env_logger::try_init());

  //   let mine_gltf = match load("tests/complete.glb", true) {
  //     Ok(mine_gltf) => {
  //       println!("Complete loaded!");
  //       mine_gltf
  //     }
  //     Err(e) => panic!("Complete: failed to load. {}", e),
  //   };
  //   assert_eq!(mine_gltf.scenes.len(), 1);
  //   let scene = &mine_gltf.scenes[0];
  //   for model in scene.models.iter() {
  //     match model.mode() {
  //       Mode::Triangles | Mode::TriangleFan | Mode::TriangleStrip => {
  //         assert!(model.triangles().is_ok());
  //       }
  //       Mode::Lines | Mode::LineLoop | Mode::LineStrip => {
  //         assert!(model.lines().is_ok());
  //       }
  //       Mode::Points => {
  //         assert!(model.points().is_ok());
  //       }
  //     }
  //   }
  // }

  // #[test]
  // fn check_cube_gltf() {
  //   drop(env_logger::try_init());

  //   let _ = match load("tests/cube_classic.gltf", true) {
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

  //   let mine_gltf = match load("tests/cube.glb", true) {
  //     Ok(mine_gltf) => {
  //       println!("cube loaded!");
  //       mine_gltf
  //     }
  //     Err(e) => panic!("cube: failed to load. {}", e),
  //   };
  //   let scene = &mine_gltf.scenes[0];
  //   let model = &scene.models[0];
  //   assert!(model.has_normals());
  //   assert!(model.has_tex_coords());
  //   assert!(model.has_tangents());
  //   for t in match model.triangles() {
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
  // fn check_material() {
  //   drop(env_logger::try_init());

  //   let mine_gltf = match load("tests/head.glb", true) {
  //     Ok(mine_gltf) => {
  //       println!("head loaded!");
  //       mine_gltf
  //     }
  //     Err(e) => panic!("cube: failed to load. {}", e),
  //   };
  //   let scene = &mine_gltf.scenes[0];
  //   let mat = match scene.models[0].material.as_ref() {
  //     Some(mat) => mat,
  //     None => panic!("Failed to load material for head."),
  //   };
  //   assert!(mat.pbr.base_color_texture.is_some());
  //   assert_eq!(mat.pbr.metallic_factor, 0.);
  // }

  // #[test]
  // fn check_invalid_path() {
  //   drop(env_logger::try_init());

  //   assert!(load("tests/invalid.glb", true).is_err());
  // }

  // #[test]
  // fn load_snowman() {
  //   drop(env_logger::try_init());

  //   let mine_gltf = match load("tests/snowman.gltf", false) {
  //     Ok(mine_gltf) => {
  //       println!("Snowman loaded!");
  //       mine_gltf
  //     }
  //     Err(e) => panic!("Snowman: failed to load. {}", e),
  //   };

  //   match mine_gltf.scenes.first() {
  //     Some(scene) => assert_eq!(scene.models.len(), 5),
  //     None => panic!("Snowman: has no scenes."),
  //   }
  // }

  #[test]
  fn test_the_spider_animations() {
    drop(env_logger::try_init());

    let spider = match load("tests/fixed_spider.glb") {
      Ok(mine_gltf) => {
        println!("spider loaded!");
        mine_gltf
      }
      Err(e) => panic!("spider: failed to load. {}", e),
    };

    assert!(!spider.bone_animations.is_empty());

    let animations = spider.bone_animations;

    println!("spider animations: {},", animations.len());

    let scene = match spider.scenes.first() {
      Some(scene) => scene,
      None => panic!("Spider has no scenes!"),
    };

    // let weights = match &scene.weights {
    //   Some(weights) => weights,
    //   None => panic!("Spider has no weights!"),
    // };

    // let keyframe = animation.keyframes;
  }

  // #[test]
  // fn test_sam() {
  //   drop(env_logger::try_init());

  //   let sam = match load("tests/minetest_sam.gltf", true) {
  //     Ok(mine_gltf) => {
  //       println!("sam loaded!");
  //       mine_gltf
  //     }
  //     Err(e) => panic!("minetest_sam: failed to load. {}", e),
  //   };

  //   assert!(!sam.bone_animations.is_empty());

  //   let animations = sam.bone_animations;

  //   println!("sam animations: {},", animations.len());

  //   let scene = match sam.scenes.first() {
  //     Some(scene) => scene,
  //     None => panic!("sam has no scenes!"),
  //   };

  //   let weights = match &scene.weights {
  //     Some(weights) => weights,
  //     None => panic!("sam has no weights!"),
  //   };
  // }

  // #[test]
  // fn test_engine() {
  //   drop(env_logger::try_init());

  //   let gearbox = match load("tests/gearbox.gltf", true) {
  //     Ok(mine_gltf) => {
  //       println!("engine loaded!");
  //       mine_gltf
  //     }
  //     Err(e) => panic!("gearbox: failed to load. {}", e),
  //   };

  //   assert!(!gearbox.bone_animations.is_empty());

  //   let animations = gearbox.bone_animations;

  //   println!("gearbox animations: {},", animations.len());

  //   let scene = match gearbox.scenes.first() {
  //     Some(scene) => scene,
  //     None => panic!("gearbox has no scenes!"),
  //   };

  //   let weights = match &scene.weights {
  //     Some(weights) => weights,
  //     None => panic!("gearbox has no weights!"),
  //   };
  // }

  // #[test]
  // fn test_brain_stem() {
  //   drop(env_logger::try_init());

  //   let gearbox = match load("tests/brain_stem.gltf", true) {
  //     Ok(mine_gltf) => {
  //       println!("brain_stem loaded!");
  //       mine_gltf
  //     }
  //     Err(e) => panic!("brain_stem: failed to load. {}", e),
  //   };

  //   assert!(!gearbox.bone_animations.is_empty());

  //   let animations = gearbox.bone_animations;

  //   println!("brain_stem animations: {},", animations.len());

  //   let scene = match gearbox.scenes.first() {
  //     Some(scene) => scene,
  //     None => panic!("brain_stem has no scenes!"),
  //   };

  //   // let weights = match &scene.weights {
  //   //   Some(weights) => weights,
  //   //   None => panic!("brain_stem has no weights!"),
  //   // };
  // }

  // #[test]
  // fn load_simple_skin() {
  //   drop(env_logger::try_init());

  //   let mine_gltf = match load("tests/simple_skin.gltf", false) {
  //     Ok(mine_gltf) => {
  //       println!("simple_skin loaded!");
  //       mine_gltf
  //     }
  //     Err(e) => panic!("simple_skin: failed to load. {}", e),
  //   };

  //   let scene = match mine_gltf.scenes.first() {
  //     Some(scene) => scene,
  //     None => panic!("simple_skin: has no scenes."),
  //   };

  //   // let weights = match &scene.weights {
  //   //   Some(weights) => weights,
  //   //   None => panic!("simple_skin has no weights!"),
  //   // };

  //   // This one's a curve ball. This is an ultra simple model so let's see if tries to iterate more than one channel!
  //   for (_, channel) in mine_gltf.bone_animations {
  //     assert!(
  //       channel.translation_timestamps.len() == channel.translations.len()
  //         && channel.translations.is_empty()
  //     );

  //     assert!(
  //       channel.rotation_timestamps.len() == channel.rotations.len()
  //         && channel.rotations.len() == 12
  //     );

  //     assert!(channel.scale_timestamps.len() == channel.scales.len() && channel.scales.is_empty());

  //     assert!(
  //       channel.weight_timestamps.len() == channel.weights.len() && channel.weights.is_empty()
  //     );
  //   }
  // }
}
