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
use float_cmp::{approx_eq, Ulps};
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

  if false {
    if !is_skinned {
      error!("Animation failure on model {}. >:(", file_name);
    } else {
      error!("Model {} is animated. :)", file_name);
    }
  }

  // Now apply the data.
  if is_skinned {
    // We're going to take the raw data.
    let bone_animations = grab_animations(gltf_data, buffers, file_name);

    // Then finalize it.
    // (finalization is interpolating the frames so they're all equal distance from eachother in the scale of time.)
    // todo: turn this into a function so it's not a mess here.

    // Chuck this into a scope so we can have immutable values.
    let (_min_time, max_time, min_distance) = {
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

    // println!(
    //   "min_time: {}\nmax_time: {}\nmin_distance: {}\nrequired_frames: {}",
    //   min_time, max_time, min_distance, required_frames
    // );

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
        // error!("hit none");
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
        // error!("hit one");
        let polyfill = match animation.translations.first() {
          Some(translation) => translation,
          None => panic!("translation was already checked, why did this panic!? 1"),
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

        // error!("Hit another?");
        // println!("got: {}", animation.translation_timestamps.len());
        // println!("got: {}", animation.translations.len());

        let mut raw_add = false;

        // Let's see if we can take the easist route with start to finish polyfill.
        match animation.translation_timestamps.first() {
          Some(first_timestamp) => {
            if into_precision(*first_timestamp) == 0 {
              match animation.translation_timestamps.last() {
                Some(last_timestamp) => {
                  if into_precision(*last_timestamp) == into_precision(max_time) {
                    raw_add = true;
                  }
                }
                None => panic!("translation was already checked, why did this panic!? 2"),
              }
            }
          }
          None => panic!("translation was already checked, why did this panic!? 3"),
        }

        // Now if we can raw add let's see if we can just dump the raw frames in because they're finalized.
        if raw_add && animation.translation_timestamps.len() == required_frames {
          // We can!
          // error!("OKAY TO RAW ADD!");
          new_finalized_channel.translation_timestamps = animation.translation_timestamps.clone();
          new_finalized_channel.translations = animation.translations.clone();
        } else if raw_add && animation.translation_timestamps.len() == 2 {
          // But if we only have the start and finish, we now have to polyfill between beginning and end.
          // error!("POLYFILLING FROM START TO FINISH!");
          let start = match animation.translations.first() {
            Some(start) => start,
            None => panic!("translation was already checked, why did this panic!? 4"),
          };
          let finish = match animation.translations.last() {
            Some(finish) => finish,
            None => panic!("translation was already checked, why did this panic!? 5"),
          };

          for i in 0..required_frames {
            // 0.0 to 1.0.
            let current_percentile = i as f32 / (required_frames - 1) as f32;
            // 0.0 to X max time.
            let current_stamp = current_percentile * max_time;

            // println!("current: {}", current_stamp);

            let result = start.lerp(*finish, current_percentile);

            // println!("result: {:?}", result);

            new_finalized_channel
              .translation_timestamps
              .push(current_stamp);
            new_finalized_channel.translations.push(result);
          }
        } else {
          // And if we can't do either of those, now we have to brute force our way through the polyfill calculations. :(

          // To begin this atrocity let's start by grabbing the current size of the animation container.
          let old_frame_size = animation.translation_timestamps.len();

          // This gives me great pain.
          for i in 0..required_frames {
            // 0.0 to 1.0.
            let current_percentile = i as f32 / (required_frames - 1) as f32;
            // 0.0 to X max time.
            let current_stamp = current_percentile * max_time;
            // 5 points of precision integral positioning.
            let precise_stamp = into_precision(current_stamp);

            // Okay now that we got our data, let's see if this model has it.
            // We need index ONLY cause we have to walk back and forth.
            // There might be a logic thing missing in here. If you find it. Halp.
            // ? Fun begins here.
            let mut found_frame_key = None;

            // Let's find if we have a frame that already exists in the animation.
            for i in 0..old_frame_size {
              let gotten = animation.translation_timestamps[i];

              let gotten_precise = into_precision(gotten);

              // We got lucky and found an existing frame! :D
              if gotten_precise == precise_stamp {
                found_frame_key = Some(i);
                break;
              }

              // And if this loop completes and we didn't find anything. We gotta get creative.
            }

            // If it's none we now have to either interpolate this thing or we have to insert it.
            if found_frame_key.is_none() {
              // If there's no starting keyframe.
              // First of all, why is this allowed?
              // Second of all, polyfill from the next available frame.
              // We know this thing has more than 2 available frames at this point.
              if precise_stamp == 0 {
                new_finalized_channel
                  .translation_timestamps
                  .push(current_stamp);
                // If this crashes, there's something truly horrible that has happened.
                new_finalized_channel
                  .translations
                  .push(animation.translations[1]);
              } else {
                // Else we're going to have to figure this mess out.
                // ! Here is where the program performance just tanks.

                // ? So we have no direct frame, we have to find out 2 things:
                // ? 1.) The leading frame.
                // ? 2.) The following frame.
                // ? Then we have to interpolate them together.

                // This is an option because if it's none, we have to brute force with animation frame 0.
                let mut leading_frame = None;

                for i in 0..old_frame_size {
                  let gotten = animation.translation_timestamps[i];

                  let gotten_precise = into_precision(gotten);

                  // Here we check for a frame that is less than goal.
                  // aka, the leading frame.
                  // We already checked if it's got an equal to frame, there's only unequal to frames now.
                  // We need to let this keep going until it overshoots or else it won't be accurate.
                  if gotten_precise < precise_stamp {
                    leading_frame = Some(i);
                  } else {
                    // We overshot, now time to abort.
                    break;
                  }
                }

                // ! If we have no leading leading frame is now whatever is first.
                if leading_frame.is_none() {
                  leading_frame = Some(0);
                }

                // This is an option because if it's none, we have to brute force with animation frame 0.
                let mut following_frame = None;

                for i in 0..old_frame_size {
                  let gotten = animation.translation_timestamps[i];

                  let gotten_precise = into_precision(gotten);

                  // Here we check for a frame that is less than goal.
                  // aka, the leading frame.
                  // We already checked if it's got an equal to frame, there's only unequal to frames now.
                  // We need to let this keep going until it overshoots or else it won't be accurate.
                  if gotten_precise > precise_stamp {
                    following_frame = Some(i);
                  }

                  // Can't do a logic gate in the previous statement. If it's found then break.
                  if following_frame.is_some() {
                    break;
                  }
                }

                // ? If it's none, the safe fallback is to just equalize the start and finish, which is extremely wrong.
                if following_frame.is_none() {
                  following_frame = leading_frame;
                }

                // Now we do the interpolation.
                // This isn't perfect, but it's something.
                match leading_frame {
                  Some(leader) => match following_frame {
                    Some(follower) => {
                      let lead_timestamp = animation.translation_timestamps[leader];
                      let lead_translation = animation.translations[leader];

                      let follow_timestamp = animation.translation_timestamps[follower];
                      let follow_translation = animation.translations[follower];

                      // This is a simple zeroing out of the scales.
                      let scale = follow_timestamp - lead_timestamp;

                      // Shift the current timestamp into the range of our work.
                      let shifted_stamp = current_stamp - lead_timestamp;

                      // Get it into 0.0 - 1.0.
                      let finalized_percentile = shifted_stamp / scale;

                      // println!("finalized: {}", finalized_percentile);

                      let finalized_translation_interpolation =
                        lead_translation.lerp(follow_translation, finalized_percentile);

                      // Now we finally push the interpolated translation into the finalized animation channel.
                      new_finalized_channel
                        .translations
                        .push(finalized_translation_interpolation);
                      new_finalized_channel
                        .translation_timestamps
                        .push(current_stamp);
                    }
                    None => panic!("how?!"),
                  },
                  None => panic!("how?!"),
                }
              }
            } else {
              // ! We found a keyframe! :D
              // If it's some we have an existing good frame, work with it.
              let key = match found_frame_key {
                Some(key) => key,
                None => panic!("how is that even possible?!"),
              };

              // This should never blow up. That's immutable data it's working with, within range!
              new_finalized_channel
                .translation_timestamps
                .push(animation.translation_timestamps[key]);

              new_finalized_channel
                .translations
                .push(animation.translations[key]);
            }

            // println!("test: {:?}", found_frame_key);

            // println!("{} {}", current_stamp, precise_stamp);
          }

          // panic!("minetest-gltf: This translation logic branch is disabled because I have no model that has this available yet. If this is hit. Give me your model.")
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

      // println!("t: {:?}", new_finalized_channel.translations);
      // println!("t: {:?}", new_finalized_channel.translation_timestamps);

      // println!("-=-=-=-=-");

      // ? ////////////////////////////////////////////////////////////
      // ?            ROTATIONS
      // ? ////////////////////////////////////////////////////////////

      // Final check for rotation equality.
      if animation.rotation_timestamps.len() != animation.rotations.len() {
        return Err(format!("Unequal animation rotation lengths in channel {}.", id).into());
      }

      if animation.rotation_timestamps.is_empty() {
        // error!("hit none");
        // If it's blank, we want to polyfill in default data.
        for i in 0..required_frames {
          new_finalized_channel
            .rotation_timestamps
            .push(i as f32 * min_distance);
          new_finalized_channel.rotations.push(Quat::IDENTITY);
        }
      } else if animation.rotation_timestamps.len() == 1 {
        // If there's only one, we can simply use the one rotation point as the entire rotation animation.
        // error!("hit one");
        let polyfill = match animation.rotations.first() {
          Some(rotation) => rotation,
          None => panic!("rotation was already checked, why did this panic!? 1"),
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

        // error!("Hit another?");
        // println!("got: {}", animation.rotation_timestamps.len());
        // println!("got: {}", animation.rotations.len());

        let mut raw_add = false;

        // Let's see if we can take the easist route with start to finish polyfill.
        match animation.rotation_timestamps.first() {
          Some(first_timestamp) => {
            if into_precision(*first_timestamp) == 0 {
              match animation.rotation_timestamps.last() {
                Some(last_timestamp) => {
                  if into_precision(*last_timestamp) == into_precision(max_time) {
                    raw_add = true;
                  }
                }
                None => panic!("rotation was already checked, why did this panic!? 2"),
              }
            }
          }
          None => panic!("rotation was already checked, why did this panic!? 3"),
        }

        // Now if we can raw add let's see if we can just dump the raw frames in because they're finalized.
        if raw_add && animation.rotation_timestamps.len() == required_frames {
          // We can!
          // error!("OKAY TO RAW ADD!");
          new_finalized_channel.rotation_timestamps = animation.rotation_timestamps.clone();
          new_finalized_channel.rotations = animation.rotations.clone();
        } else if raw_add && animation.rotation_timestamps.len() == 2 {
          // But if we only have the start and finish, we now have to polyfill between beginning and end.
          // error!("POLYFILLING FROM START TO FINISH!");
          let start = match animation.rotations.first() {
            Some(start) => start,
            None => panic!("rotation was already checked, why did this panic!? 4"),
          };
          let finish = match animation.rotations.last() {
            Some(finish) => finish,
            None => panic!("rotation was already checked, why did this panic!? 5"),
          };

          for i in 0..required_frames {
            // 0.0 to 1.0.
            let current_percentile = i as f32 / (required_frames - 1) as f32;
            // 0.0 to X max time.
            let current_stamp = current_percentile * max_time;

            // println!("current: {}", current_stamp);

            let result = start.lerp(*finish, current_percentile);

            // println!("result: {:?}", result);

            new_finalized_channel
              .rotation_timestamps
              .push(current_stamp);
            new_finalized_channel.rotations.push(result);
          }
        } else {
          // And if we can't do either of those, now we have to brute force our way through the polyfill calculations. :(

          // To begin this atrocity let's start by grabbing the current size of the animation container.
          let old_frame_size = animation.rotation_timestamps.len();

          // This gives me great pain.
          for i in 0..required_frames {
            // 0.0 to 1.0.
            let current_percentile = i as f32 / (required_frames - 1) as f32;
            // 0.0 to X max time.
            let current_stamp = current_percentile * max_time;
            // 5 points of precision integral positioning.
            let precise_stamp = into_precision(current_stamp);

            // Okay now that we got our data, let's see if this model has it.
            // We need index ONLY cause we have to walk back and forth.
            // There might be a logic thing missing in here. If you find it. Halp.
            // ? Fun begins here.
            let mut found_frame_key = None;

            // Let's find if we have a frame that already exists in the animation.
            for i in 0..old_frame_size {
              let gotten = animation.rotation_timestamps[i];

              let gotten_precise = into_precision(gotten);

              // We got lucky and found an existing frame! :D
              if gotten_precise == precise_stamp {
                found_frame_key = Some(i);
                break;
              }

              // And if this loop completes and we didn't find anything. We gotta get creative.
            }

            // If it's none we now have to either interpolate this thing or we have to insert it.
            if found_frame_key.is_none() {
              // If there's no starting keyframe.
              // First of all, why is this allowed?
              // Second of all, polyfill from the next available frame.
              // We know this thing has more than 2 available frames at this point.
              if precise_stamp == 0 {
                new_finalized_channel
                  .rotation_timestamps
                  .push(current_stamp);
                // If this crashes, there's something truly horrible that has happened.
                new_finalized_channel.rotations.push(animation.rotations[1]);
              } else {
                // Else we're going to have to figure this mess out.
                // ! Here is where the program performance just tanks.

                // ? So we have no direct frame, we have to find out 2 things:
                // ? 1.) The leading frame.
                // ? 2.) The following frame.
                // ? Then we have to interpolate them together.

                // This is an option because if it's none, we have to brute force with animation frame 0.
                let mut leading_frame = None;

                for i in 0..old_frame_size {
                  let gotten = animation.rotation_timestamps[i];

                  let gotten_precise = into_precision(gotten);

                  // Here we check for a frame that is less than goal.
                  // aka, the leading frame.
                  // We already checked if it's got an equal to frame, there's only unequal to frames now.
                  // We need to let this keep going until it overshoots or else it won't be accurate.
                  if gotten_precise < precise_stamp {
                    leading_frame = Some(i);
                  } else {
                    // We overshot, now time to abort.
                    break;
                  }
                }

                // ! If we have no leading leading frame is now whatever is first.
                if leading_frame.is_none() {
                  leading_frame = Some(0);
                }

                // This is an option because if it's none, we have to brute force with animation frame 0.
                let mut following_frame = None;

                for i in 0..old_frame_size {
                  let gotten = animation.rotation_timestamps[i];

                  let gotten_precise = into_precision(gotten);

                  // Here we check for a frame that is less than goal.
                  // aka, the leading frame.
                  // We already checked if it's got an equal to frame, there's only unequal to frames now.
                  // We need to let this keep going until it overshoots or else it won't be accurate.
                  if gotten_precise > precise_stamp {
                    following_frame = Some(i);
                  }

                  // Can't do a logic gate in the previous statement. If it's found then break.
                  if following_frame.is_some() {
                    break;
                  }
                }

                // ? If it's none, the safe fallback is to just equalize the start and finish, which is extremely wrong.
                if following_frame.is_none() {
                  following_frame = leading_frame;
                }

                // Now we do the interpolation.
                // This isn't perfect, but it's something.
                match leading_frame {
                  Some(leader) => match following_frame {
                    Some(follower) => {
                      let lead_timestamp = animation.rotation_timestamps[leader];
                      let lead_rotation = animation.rotations[leader];

                      let follow_timestamp = animation.rotation_timestamps[follower];
                      let follow_rotation = animation.rotations[follower];

                      // This is a simple zeroing out of the scales.
                      let scale = follow_timestamp - lead_timestamp;

                      // Shift the current timestamp into the range of our work.
                      let shifted_stamp = current_stamp - lead_timestamp;

                      // Get it into 0.0 - 1.0.
                      let finalized_percentile = shifted_stamp / scale;

                      // println!("finalized: {}", finalized_percentile);

                      let finalized_rotation_interpolation =
                        lead_rotation.lerp(follow_rotation, finalized_percentile);

                      // Now we finally push the interpolated rotation into the finalized animation channel.
                      new_finalized_channel
                        .rotations
                        .push(finalized_rotation_interpolation);
                      new_finalized_channel
                        .rotation_timestamps
                        .push(current_stamp);
                    }
                    None => panic!("how?!"),
                  },
                  None => panic!("how?!"),
                }
              }
            } else {
              // ! We found a keyframe! :D
              // If it's some we have an existing good frame, work with it.
              let key = match found_frame_key {
                Some(key) => key,
                None => panic!("how is that even possible?!"),
              };

              // This should never blow up. That's immutable data it's working with, within range!
              new_finalized_channel
                .rotation_timestamps
                .push(animation.rotation_timestamps[key]);

              new_finalized_channel
                .rotations
                .push(animation.rotations[key]);
            }

            // println!("test: {:?}", found_frame_key);

            // println!("{} {}", current_stamp, precise_stamp);
          }

          // panic!("minetest-gltf: This rotation logic branch is disabled because I have no model that has this available yet. If this is hit. Give me your model.")
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

      // println!("t: {:?}", new_finalized_channel.rotations);
      // println!("t: {:?}", new_finalized_channel.rotation_timestamps);

      // ? ////////////////////////////////////////////////////////////
      // ?            SCALES
      // ? ////////////////////////////////////////////////////////////

      // Final check for scale equality.
      if animation.scale_timestamps.len() != animation.scales.len() {
        return Err(format!("Unequal animation scale lengths in channel {}.", id).into());
      }

      if animation.scale_timestamps.is_empty() {
        // error!("hit none");
        // If it's blank, we want to polyfill in default data.
        for i in 0..required_frames {
          new_finalized_channel
            .scale_timestamps
            .push(i as f32 * min_distance);
          new_finalized_channel.scales.push(Vec3::new(1.0, 1.0, 1.0));
        }
      } else if animation.scale_timestamps.len() == 1 {
        // If there's only one, we can simply use the one scale point as the entire scale animation.
        // error!("hit one");
        let polyfill = match animation.scales.first() {
          Some(scale) => scale,
          None => panic!("scale was already checked, why did this panic!? 1"),
        };

        for i in 0..required_frames {
          new_finalized_channel
            .scale_timestamps
            .push(i as f32 * min_distance);
          new_finalized_channel.scales.push(*polyfill);
        }
      } else {
        // Now if we can't polyfill with the easiest data set,
        // we're going to have to get creative.

        // error!("Hit another?");
        // println!("got: {}", animation.scale_timestamps.len());
        // println!("got: {}", animation.scales.len());

        let mut raw_add = false;

        // Let's see if we can take the easist route with start to finish polyfill.
        match animation.scale_timestamps.first() {
          Some(first_timestamp) => {
            if into_precision(*first_timestamp) == 0 {
              match animation.scale_timestamps.last() {
                Some(last_timestamp) => {
                  if into_precision(*last_timestamp) == into_precision(max_time) {
                    raw_add = true;
                  }
                }
                None => panic!("scale was already checked, why did this panic!? 2"),
              }
            }
          }
          None => panic!("scale was already checked, why did this panic!? 3"),
        }

        // Now if we can raw add let's see if we can just dump the raw frames in because they're finalized.
        if raw_add && animation.scale_timestamps.len() == required_frames {
          // We can!
          // error!("OKAY TO RAW ADD!");
          new_finalized_channel.scale_timestamps = animation.scale_timestamps.clone();
          new_finalized_channel.scales = animation.scales.clone();
        } else if raw_add && animation.scale_timestamps.len() == 2 {
          // But if we only have the start and finish, we now have to polyfill between beginning and end.
          // error!("POLYFILLING FROM START TO FINISH!");
          let start = match animation.scales.first() {
            Some(start) => start,
            None => panic!("scale was already checked, why did this panic!? 4"),
          };
          let finish = match animation.scales.last() {
            Some(finish) => finish,
            None => panic!("scale was already checked, why did this panic!? 5"),
          };

          for i in 0..required_frames {
            // 0.0 to 1.0.
            let current_percentile = i as f32 / (required_frames - 1) as f32;
            // 0.0 to X max time.
            let current_stamp = current_percentile * max_time;

            // println!("current: {}", current_stamp);

            let result = start.lerp(*finish, current_percentile);

            // println!("result: {:?}", result);

            new_finalized_channel.scale_timestamps.push(current_stamp);
            new_finalized_channel.scales.push(result);
          }
        } else {
          // And if we can't do either of those, now we have to brute force our way through the polyfill calculations. :(

          // To begin this atrocity let's start by grabbing the current size of the animation container.
          let old_frame_size = animation.scale_timestamps.len();

          // This gives me great pain.
          for i in 0..required_frames {
            // 0.0 to 1.0.
            let current_percentile = i as f32 / (required_frames - 1) as f32;
            // 0.0 to X max time.
            let current_stamp = current_percentile * max_time;
            // 5 points of precision integral positioning.
            let precise_stamp = into_precision(current_stamp);

            // Okay now that we got our data, let's see if this model has it.
            // We need index ONLY cause we have to walk back and forth.
            // There might be a logic thing missing in here. If you find it. Halp.
            // ? Fun begins here.
            let mut found_frame_key = None;

            // Let's find if we have a frame that already exists in the animation.
            for i in 0..old_frame_size {
              let gotten = animation.scale_timestamps[i];

              let gotten_precise = into_precision(gotten);

              // We got lucky and found an existing frame! :D
              if gotten_precise == precise_stamp {
                found_frame_key = Some(i);
                break;
              }

              // And if this loop completes and we didn't find anything. We gotta get creative.
            }

            // If it's none we now have to either interpolate this thing or we have to insert it.
            if found_frame_key.is_none() {
              // If there's no starting keyframe.
              // First of all, why is this allowed?
              // Second of all, polyfill from the next available frame.
              // We know this thing has more than 2 available frames at this point.
              if precise_stamp == 0 {
                new_finalized_channel.scale_timestamps.push(current_stamp);
                // If this crashes, there's something truly horrible that has happened.
                new_finalized_channel.scales.push(animation.scales[1]);
              } else {
                // Else we're going to have to figure this mess out.
                // ! Here is where the program performance just tanks.

                // ? So we have no direct frame, we have to find out 2 things:
                // ? 1.) The leading frame.
                // ? 2.) The following frame.
                // ? Then we have to interpolate them together.

                // This is an option because if it's none, we have to brute force with animation frame 0.
                let mut leading_frame = None;

                for i in 0..old_frame_size {
                  let gotten = animation.scale_timestamps[i];

                  let gotten_precise = into_precision(gotten);

                  // Here we check for a frame that is less than goal.
                  // aka, the leading frame.
                  // We already checked if it's got an equal to frame, there's only unequal to frames now.
                  // We need to let this keep going until it overshoots or else it won't be accurate.
                  if gotten_precise < precise_stamp {
                    leading_frame = Some(i);
                  } else {
                    // We overshot, now time to abort.
                    break;
                  }
                }

                // ! If we have no leading leading frame is now whatever is first.
                if leading_frame.is_none() {
                  leading_frame = Some(0);
                }

                // This is an option because if it's none, we have to brute force with animation frame 0.
                let mut following_frame = None;

                for i in 0..old_frame_size {
                  let gotten = animation.scale_timestamps[i];

                  let gotten_precise = into_precision(gotten);

                  // Here we check for a frame that is less than goal.
                  // aka, the leading frame.
                  // We already checked if it's got an equal to frame, there's only unequal to frames now.
                  // We need to let this keep going until it overshoots or else it won't be accurate.
                  if gotten_precise > precise_stamp {
                    following_frame = Some(i);
                  }

                  // Can't do a logic gate in the previous statement. If it's found then break.
                  if following_frame.is_some() {
                    break;
                  }
                }

                // ? If it's none, the safe fallback is to just equalize the start and finish, which is extremely wrong.
                if following_frame.is_none() {
                  following_frame = leading_frame;
                }

                // Now we do the interpolation.
                // This isn't perfect, but it's something.
                match leading_frame {
                  Some(leader) => match following_frame {
                    Some(follower) => {
                      let lead_timestamp = animation.scale_timestamps[leader];
                      let lead_scale = animation.scales[leader];

                      let follow_timestamp = animation.scale_timestamps[follower];
                      let follow_scale = animation.scales[follower];

                      // This is a simple zeroing out of the scales.
                      let scale = follow_timestamp - lead_timestamp;

                      // Shift the current timestamp into the range of our work.
                      let shifted_stamp = current_stamp - lead_timestamp;

                      // Get it into 0.0 - 1.0.
                      let finalized_percentile = shifted_stamp / scale;

                      // println!("finalized: {}", finalized_percentile);

                      let finalized_scale_interpolation =
                        lead_scale.lerp(follow_scale, finalized_percentile);

                      // Now we finally push the interpolated scale into the finalized animation channel.
                      new_finalized_channel
                        .scales
                        .push(finalized_scale_interpolation);
                      new_finalized_channel.scale_timestamps.push(current_stamp);
                    }
                    None => panic!("how?!"),
                  },
                  None => panic!("how?!"),
                }
              }
            } else {
              // ! We found a keyframe! :D
              // If it's some we have an existing good frame, work with it.
              let key = match found_frame_key {
                Some(key) => key,
                None => panic!("how is that even possible?!"),
              };

              // This should never blow up. That's immutable data it's working with, within range!
              new_finalized_channel
                .scale_timestamps
                .push(animation.scale_timestamps[key]);

              new_finalized_channel.scales.push(animation.scales[key]);
            }

            // println!("test: {:?}", found_frame_key);

            // println!("{} {}", current_stamp, precise_stamp);
          }

          // panic!("minetest-gltf: This scale logic branch is disabled because I have no model that has this available yet. If this is hit. Give me your model.")
        }
      }

      if new_finalized_channel.scale_timestamps.len() != new_finalized_channel.scales.len() {
        panic!("BLEW UP! Mismatched scale lengths.");
      }
      if new_finalized_channel.scale_timestamps.len() != required_frames {
        panic!(
          "BLEW UP! scale frames Expected: {} got: {}",
          required_frames,
          new_finalized_channel.scale_timestamps.len()
        );
      }

      // println!("t: {:?}", new_finalized_channel.scales);
      // println!("t: {:?}", new_finalized_channel.scale_timestamps);

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
/// We need a comparable data set. Cast this this thing 0.00001 f32 5 precision points into 1 i32
///
fn into_precision(x: f32) -> i32 {
  (x * 100_000.0) as i32
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
