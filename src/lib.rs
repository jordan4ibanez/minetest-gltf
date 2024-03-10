// ! This crate is intended to load [glTF 2.0](https://www.khronos.org/gltf), a
// ! file format designed for the efficient transmission of 3D assets.
// !
// ! It's base on [gltf](https://github.com/gltf-rs/gltf) crate but has an easy to use output.
// !
// ! # Installation
// !
// ! ```toml
// ! [dependencies]
// ! easy-gltf="1.1.1"
// ! ```
// !
// ! # Example
// !
// ! ```
// ! let mine_gltf = minetest_gltf::load("tests/cube.glb", true).expect("Failed to load glTF");
// ! for scene in mine_gltf.scenes {
// !     println!(
// !         "Models: #{}",
// !         scene.models.len()
// !     )
// ! }
// ! ```

//!
//! I am a crate, wow.
//!
mod minetest_gltf;
mod model;

use ahash::AHashMap;
use glam::{Quat, Vec3};
use gltf::Gltf;
use log::error;
use minetest_gltf::MinetestGLTF;
use model::animation::{grab_animations, BoneAnimationChannel};
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
  if !is_skinned {
    error!("Animation failure on model {}. >:(", file_name);
  } else {
    error!("Model {} is animated. :)", file_name);
  }

  // Now apply the data.
  if is_skinned {
    // We're going to take the raw data.
    let bone_animations = grab_animations(gltf_data, buffers, file_name);

    // Then finalize it.
    // (finalization is interpolating the frames so they're all equal distance from eachother in the scale of time.)
    // todo: turn this into a function so it's not a mess here.

    // Chuck this into a scope so we can have immutable values.
    let (min_time, max_time, min_distance) = {
      let mut min_time_worker = 0.0;
      let mut max_time_worker = 0.0;
      let mut min_distance_worker = f32::MAX;

      for (_id, animation) in &bone_animations {
        // A closure so I don't have to type this out 4 times.
        let mut devolve_timestamp_data = |raw_timestamps: &Vec<f32>| {
          let mut old_timestamp = f32::MIN;
          for timestamp in raw_timestamps {
            // Time distance data.
            if *timestamp - old_timestamp < min_distance_worker {
              min_distance_worker = *timestamp - old_timestamp;
            }

            // Min time data.
            if timestamp < &min_time_worker {
              min_time_worker = *timestamp;
            }
            // Max time data.
            if timestamp > &max_time_worker {
              max_time_worker = *timestamp;
            }

            old_timestamp = *timestamp;
          }
        };

        // Translation timestamps.
        devolve_timestamp_data(&animation.translation_timestamps);

        // Rotation timestamps.
        devolve_timestamp_data(&animation.rotation_timestamps);

        // Scale timestamps.
        devolve_timestamp_data(&animation.rotation_timestamps);

        // Weight timestamps.
        devolve_timestamp_data(&animation.weight_timestamps);
      }

      (min_time_worker, max_time_worker, min_distance_worker)
    };

    // Now we need a triple checker variable.
    // We need to make sure that all the channels have this many frames.
    // This will also work as an iterator.
    // Timestamps start at 0.0. That's why it's + 1. It's a zero counted container.
    let required_frames = (max_time / min_distance).round() as usize + 1;

    println!(
      "min_time: {}\nmax_time: {}\nmin_distance: {}\nrequired_frames: {}",
      min_time, max_time, min_distance, required_frames
    );

    let enable_timestamp_spam = false;

    if enable_timestamp_spam {
      for i in 0..required_frames {
        println!("test: {}", i as f32 * min_distance);
      }
    }

    // Now we finalize all animation channels.
    let mut finalized_bone_animations: AHashMap<i32, BoneAnimationChannel> = AHashMap::new();

    for (id, animation) in &bone_animations {
      // ! This is going to get a bit complicated.
      // ! Like, extremely complicated.

      // Add a channel to the current id in the finalized animations container.
      let mut new_finalized_channel = BoneAnimationChannel::new();

      // ? ////////////////////////////////////////////////////////////
      // ?            TRANSLATIONS
      // ? ////////////////////////////////////////////////////////////

      // Final check for translation equality.
      if animation.translation_timestamps.len() != animation.translations.len() {
        return Err(format!("Unequal animation translation lengths in channel {}.", id).into());
      }

      if animation.translation_timestamps.is_empty() {
        error!("hit none");
        // If it's blank, we want to polyfill in default data.
        for i in 0..required_frames {
          new_finalized_channel
            .translation_timestamps
            .push(i as f32 * min_distance);
          new_finalized_channel
            .translations
            .push(Vec3::new(0.0, 0.0, 0.0));
        }
      } else if animation.translation_timestamps.len() == 1 {
        // If there's only one, we can simply use the one translation point as the entire translation animation.
        error!("hit one");
        let polyfill = match animation.translations.first() {
          Some(translation) => translation,
          None => panic!("what?!"),
        };

        for i in 0..required_frames {
          new_finalized_channel
            .translation_timestamps
            .push(i as f32 * min_distance);
          new_finalized_channel.translations.push(*polyfill);
        }
      } else {
        // Now if we can't polyfill with the easiest data set,
        // we're going to have to get creative.

        error!("Hit another?");
        println!("got: {}", animation.translation_timestamps.len());
        println!("got: {}", animation.translations.len());

        let mut raw_add = false;

        // Let's see if we can take the easist route with start to finish polyfill.
        match animation.translation_timestamps.first() {
          Some(first_timestamp) => {
            if *first_timestamp == 0.0 {
              match animation.translation_timestamps.last() {
                Some(last_timestamp) => {
                  if *last_timestamp == max_time {
                    raw_add = true;
                  }
                }
                None => panic!("wat1"),
              }
            }
          }
          None => panic!("wat2"),
        }

        // Now if we can raw add let's see if we can just dump the raw frames in because they're finalized.
        if raw_add && animation.translation_timestamps.len() == required_frames {
          // We can!
          error!("OKAY TO RAW ADD!");
          new_finalized_channel.translation_timestamps = animation.translation_timestamps.clone();
          new_finalized_channel.translations = animation.translations.clone();
        } else if raw_add && animation.translation_timestamps.len() == 2 {
          // But if we only have the start and finish, we now have to polyfill between beginning and end.
          error!("POLYFILLING FROM START TO FINISH!");
          let start = match animation.translations.first() {
            Some(start) => start,
            None => panic!("wat wat 1"),
          };
          let finish = match animation.translations.last() {
            Some(finish) => finish,
            None => panic!("wat wat 2"),
          };

          for i in 0..required_frames {
            // 0.0 to 1.0.
            let current_percentile = i as f32 / (required_frames - 1) as f32;
            // 0.0 to X max time.
            let current_stamp = current_percentile * max_time;

            // println!("current: {}", current_stamp);

            let result = start.lerp(*finish, current_percentile);

            println!("result: {:?}", result);

            new_finalized_channel
              .translation_timestamps
              .push(current_stamp);
            new_finalized_channel.translations.push(result);
          }
        } else {
          // And if we can't do either of those, now we have to brute force our way through the calculations. :(

          // This is disabled because I have no model that has this available yet. If this is hit. Give me your model.

          panic!("minetest-gltf: This translation logic branch is disabled because I have no model that has this available yet. If this is hit. Give me your model.")

          // for (timestamp, value) in animation
          //   .translation_timestamps
          //   .iter()
          //   .zip(&animation.translations)
          // {
          //   // println!("old timestamp: {}", old_time);
          //   if timestamp - old_time > min_distance {
          //     // println!("current timestamp: {}", timestamp);
          //     // println!("current distance: {}", timestamp - old_time);
          //     // error!("we need a polyfill in translations.");
          //     let fill_in = ((timestamp - old_time) / min_distance).round() as usize;
          //     // println!("need to fill in {} frames!", fill_in);
          //   } else {
          //     new_finalized_channel
          //       .translation_timestamps
          //       .push(*timestamp);
          //     new_finalized_channel.translations.push(*value);
          //   }

          //   old_time = *timestamp;
          // }
        }
      }

      if new_finalized_channel.translation_timestamps.len()
        != new_finalized_channel.translations.len()
      {
        panic!("BLEW UP! Mismatched translation lengths.");
      }
      if new_finalized_channel.translation_timestamps.len() != required_frames {
        panic!(
          "BLEW UP! translation frames Expected: {} got: {}",
          required_frames,
          new_finalized_channel.translation_timestamps.len()
        );
      }

      println!("t: {:?}", new_finalized_channel.translations);
      println!("t: {:?}", new_finalized_channel.translation_timestamps);

      println!("-=-=-=-=-");

      // ? ////////////////////////////////////////////////////////////
      // ?            ROTATIONS
      // ? ////////////////////////////////////////////////////////////

      // Final check for rotation equality.
      if animation.rotation_timestamps.len() != animation.rotations.len() {
        return Err(format!("Unequal animation rotation lengths in channel {}.", id).into());
      }

      if animation.rotation_timestamps.is_empty() {
        error!("hit none");
        // If it's blank, we want to polyfill in default data.
        for i in 0..required_frames {
          new_finalized_channel
            .rotation_timestamps
            .push(i as f32 * min_distance);
          new_finalized_channel.rotations.push(Quat::IDENTITY);
        }
      } else if animation.rotation_timestamps.len() == 1 {
        // If there's only one, we can simply use the one rotation point as the entire rotation animation.
        error!("hit one");
        let polyfill = match animation.rotations.first() {
          Some(rotation) => rotation,
          None => panic!("what?!"),
        };

        for i in 0..required_frames {
          new_finalized_channel
            .rotation_timestamps
            .push(i as f32 * min_distance);
          new_finalized_channel.rotations.push(*polyfill);
        }
      } else {
        // Now if we can't polyfill with the easiest data set,
        // we're going to have to get creative.

        error!("Hit another?");
        println!("got: {}", animation.rotation_timestamps.len());
        println!("got: {}", animation.rotations.len());

        let mut raw_add = false;

        // Let's see if we can take the easist route with start to finish polyfill.
        match animation.rotation_timestamps.first() {
          Some(first_timestamp) => {
            if *first_timestamp == 0.0 {
              match animation.rotation_timestamps.last() {
                Some(last_timestamp) => {
                  if *last_timestamp == max_time {
                    raw_add = true;
                  }
                }
                None => panic!("wat1"),
              }
            }
          }
          None => panic!("wat2"),
        }

        // Now if we can raw add let's see if we can just dump the raw frames in because they're finalized.
        if raw_add && animation.rotation_timestamps.len() == required_frames {
          // We can!
          error!("OKAY TO RAW ADD!");
          new_finalized_channel.rotation_timestamps = animation.rotation_timestamps.clone();
          new_finalized_channel.rotations = animation.rotations.clone();
        } else if raw_add && animation.rotation_timestamps.len() == 2 {
          // But if we only have the start and finish, we now have to polyfill between beginning and end.
          error!("POLYFILLING FROM START TO FINISH!");
          let start = match animation.rotations.first() {
            Some(start) => start,
            None => panic!("wat wat 1"),
          };
          let finish = match animation.rotations.last() {
            Some(finish) => finish,
            None => panic!("wat wat 2"),
          };

          for i in 0..required_frames {
            // 0.0 to 1.0.
            let current_percentile = i as f32 / (required_frames - 1) as f32;
            // 0.0 to X max time.
            let current_stamp = current_percentile * max_time;

            // println!("current: {}", current_stamp);

            let result = start.lerp(*finish, current_percentile);

            println!("result: {:?}", result);

            new_finalized_channel
              .rotation_timestamps
              .push(current_stamp);
            new_finalized_channel.rotations.push(result);
          }
        } else {
          // And if we can't do either of those, now we have to brute force our way through the calculations. :(

          // This is disabled because I have no model that has this available yet. If this is hit. Give me your model.

          panic!("minetest-gltf: This rotation logic branch is disabled because I have no model that has this available yet. If this is hit. Give me your model.")

          // for (timestamp, value) in animation
          //   .rotation_timestamps
          //   .iter()
          //   .zip(&animation.rotations)
          // {
          //   // println!("old timestamp: {}", old_time);
          //   if timestamp - old_time > min_distance {
          //     // println!("current timestamp: {}", timestamp);
          //     // println!("current distance: {}", timestamp - old_time);
          //     // error!("we need a polyfill in rotations.");
          //     let fill_in = ((timestamp - old_time) / min_distance).round() as usize;
          //     // println!("need to fill in {} frames!", fill_in);
          //   } else {
          //     new_finalized_channel
          //       .rotation_timestamps
          //       .push(*timestamp);
          //     new_finalized_channel.rotations.push(*value);
          //   }

          //   old_time = *timestamp;
          // }
        }
      }

      if new_finalized_channel.rotation_timestamps.len() != new_finalized_channel.rotations.len() {
        panic!("BLEW UP! Mismatched rotation lengths.");
      }
      if new_finalized_channel.rotation_timestamps.len() != required_frames {
        panic!(
          "BLEW UP! rotation frames Expected: {} got: {}",
          required_frames,
          new_finalized_channel.rotation_timestamps.len()
        );
      }

      println!("t: {:?}", new_finalized_channel.rotations);
      println!("t: {:?}", new_finalized_channel.rotation_timestamps);

      // ? ////////////////////////////////////////////////////////////
      // ?            SCALES
      // ? ////////////////////////////////////////////////////////////

      if animation.scale_timestamps.is_empty() {
        // If it's blank, we want to polyfill in default data.
        for i in 0..required_frames {
          new_finalized_channel
            .scale_timestamps
            .push(i as f32 * min_distance);
          new_finalized_channel.scales.push(Vec3::new(1.0, 1.0, 1.0));
        }
      } else {
        let mut old_time = 0.0;

        for (timestamp, value) in animation.scale_timestamps.iter().zip(&animation.scales) {
          if timestamp - old_time > min_distance {
            // error!("we need a polyfill in scales.");
          } else {
            new_finalized_channel.scale_timestamps.push(*timestamp);
            new_finalized_channel.scales.push(*value);
          }

          old_time = *timestamp;
        }
      }

      // println!("s: {:?}", new_finalized_channel.scales);
      // println!("s: {:?}", new_finalized_channel.scale_timestamps);

      // println!("-=-=-=-=-");

      // Finally add it in.
      // println!("Adding in channel: {}", id);
      finalized_bone_animations.insert(*id, new_finalized_channel);
    }

    // Then insert the finalized data here.
    minetest_gltf.bone_animations = Some(finalized_bone_animations);
    minetest_gltf.is_animated = true;
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
  // use crate::primitive::Mode;
  use crate::*;
  // use glam::Vec3;

  // macro_rules! assert_delta {
  //   ($x:expr, $y:expr, $d:expr) => {
  //     if !($x - $y < $d || $y - $x < $d) {
  //       panic!();
  //     }
  //   };
  // }

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

  //   let mine_gltf = match load("tests/complete.glb") {
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

  //   let mine_gltf = match load("tests/snowman.gltf") {
  //     Ok(mine_gltf) => {
  //       println!("Snowman loaded!");
  //       mine_gltf
  //     }
  //     Err(e) => panic!("Snowman: failed to load. {}", e),
  //   };

  //   match mine_gltf.model {
  //     Some(model) => assert_eq!(model.primitives.len(), 5),
  //     None => panic!("Snowman: has no model."),
  //   }
  // }

  // #[test]
  // fn test_the_spider_animations() {
  //   drop(env_logger::try_init());

  //   let spider = match load("tests/fixed_spider.glb") {
  //     Ok(mine_gltf) => {
  //       println!("spider loaded!");
  //       mine_gltf
  //     }
  //     Err(e) => panic!("spider: failed to load. {}", e),
  //   };

  //   assert!(!spider.bone_animations.is_empty());

  //   let animations = spider.bone_animations;

  //   println!("spider animations: {},", animations.len());

  //   let _model = match spider.model {
  //     Some(model) => model,
  //     None => panic!("Spider has no model!"),
  //   };

  //   // let weights = match &scene.weights {
  //   //   Some(weights) => weights,
  //   //   None => panic!("Spider has no weights!"),
  //   // };

  //   let keyframe_id = match animations.keys().next() {
  //     Some(keyframe_id) => keyframe_id,
  //     None => panic!("spider has no animations."),
  //   };

  //   let keyframe = match animations.get(keyframe_id) {
  //     Some(keyframe) => keyframe,
  //     None => panic!("spider has had a strange bug happen."),
  //   };

  //   println!("{:?}", keyframe.translations);
  //   println!("{:?}", keyframe.translation_timestamps);
  // }

  #[test]
  fn test_sam() {
    drop(env_logger::try_init());

    let sam = match load("tests/minetest_sam.gltf") {
      Ok(mine_gltf) => {
        println!("sam loaded!");
        mine_gltf
      }
      Err(e) => panic!("minetest_sam: failed to load. {}", e),
    };

    assert!(sam.bone_animations.is_some());

    let animations = match sam.bone_animations {
      Some(animations) => animations,
      None => panic!("sam has no bone animations!"),
    };

    println!("sam animations: {},", animations.len());

    // let weights = match &scene.weights {
    //   Some(weights) => weights,
    //   None => panic!("sam has no weights!"),
    // };
  }

  // #[test]
  // fn load_simple_skin() {
  //   drop(env_logger::try_init());

  //   let mine_gltf = match load("tests/simple_skin.gltf") {
  //     Ok(mine_gltf) => {
  //       println!("simple_skin loaded!");
  //       mine_gltf
  //     }
  //     Err(e) => panic!("simple_skin: failed to load. {}", e),
  //   };

  //   // let weights = match &scene.weights {
  //   //   Some(weights) => weights,
  //   //   None => panic!("simple_skin has no weights!"),
  //   // };

  //   // This one's a curve ball. This is an ultra simple model so let's see if tries to iterate more than one channel!
  //   match mine_gltf.bone_animations {
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
