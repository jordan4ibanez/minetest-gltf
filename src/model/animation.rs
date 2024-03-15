// Based on https://whoisryosuke.com/blog/2022/importing-gltf-with-wgpu-and-rust
// You can thank ryosuke for this information.

use std::error::Error;

use ahash::AHashMap;
use glam::{Quat, Vec3};
use gltf::{animation::util, buffer::Data, Gltf};
use log::error;

use crate::minetest_gltf::MinetestGLTF;

/// Raw animation data. Unionized.
pub enum Keyframes {
  /// Translation raw data.
  Translation(Vec<Vec3>),
  /// Rotation raw data.
  Rotation(Vec<Quat>),
  /// Scale raw data.
  Scale(Vec<Vec3>),
  /// Morph Target Weights raw data.
  Weights(Vec<f32>),
  /// An absolute failure that shows something blew up.
  Explosion,
}

/// Container containing raw TRS animation data for a node (bone).
#[derive(Default)]
pub struct BoneAnimationChannel {
  /// Translation data.
  pub translations: Vec<Vec3>,
  /// Translation timestamp data.
  pub translation_timestamps: Vec<f32>,

  /// Rotation data.
  pub rotations: Vec<Quat>,
  /// Rotation timestamp data.
  pub rotation_timestamps: Vec<f32>,

  /// Scale data.
  pub scales: Vec<Vec3>,
  /// Scale timestamp data.
  pub scale_timestamps: Vec<f32>,

  /// Weight data.
  pub weights: Vec<f32>,
  /// Not sure why you'll need this but it's here.
  ///
  /// Weight timestamp data.
  pub weight_timestamps: Vec<f32>,
}

impl BoneAnimationChannel {
  ///
  /// Create new bone animation.
  ///
  pub(crate) fn new() -> Self {
    BoneAnimationChannel {
      translations: vec![],
      translation_timestamps: vec![],
      rotations: vec![],
      rotation_timestamps: vec![],
      scales: vec![],
      scale_timestamps: vec![],
      weights: vec![],
      weight_timestamps: vec![],
    }
  }
}

///
/// We need a comparable data set. Cast this this thing 0.00001 f32 5 precision points into 1 i32
///
fn into_precision(x: f32) -> i32 {
  (x * 100_000.0) as i32
}

pub(crate) fn grab_animations(
  gltf_data: Gltf,
  buffers: Vec<Data>,
  file_name: &str,
) -> AHashMap<i32, BoneAnimationChannel> {
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
            // More advanced control flow and boilerplate reduction for when something
            // that's not implemented blows up.
            let mut blew_up = false;
            let mut generic_failure = |data_type: &str, implementation_type: &str| {
              error!(
                "Minetest_gltf: {} is not implemented for animation {}.",
                data_type, implementation_type
              );
              bone_animation_channels.clear();
              blew_up = true;
              Keyframes::Explosion
            };

            let keyframe_result = match outputs {
              util::ReadOutputs::Translations(translation) => {
                Keyframes::Translation(translation.map(Vec3::from_array).collect())
              }

              util::ReadOutputs::Rotations(rotation) => match rotation {
                util::Rotations::I8(_rotation) => generic_failure("i8", "rotation"),
                util::Rotations::U8(_rotation) => generic_failure("u8", "rotation"),
                util::Rotations::I16(_rotation) => generic_failure("i16", "rotation"),
                util::Rotations::U16(_rotation) => generic_failure("u16", "rotation"),
                util::Rotations::F32(rotation) => Keyframes::Rotation(
                  rotation
                    .map(|rot| {
                      Quat::from_array({
                        let mut returning_array: [f32; 4] = [0.0, 0.0, 0.0, 0.0];
                        for (i, v) in rot.iter().enumerate() {
                          returning_array[i] = *v;
                        }
                        returning_array
                      })
                    })
                    .collect(),
                ),
              },
              util::ReadOutputs::Scales(scale) => {
                Keyframes::Scale(scale.map(Vec3::from_array).collect())
              }
              util::ReadOutputs::MorphTargetWeights(target_weight) => match target_weight {
                util::MorphTargetWeights::I8(_weights) => {
                  generic_failure("i8", "morph weight targets")
                }
                util::MorphTargetWeights::U8(_weights) => {
                  generic_failure("u8", "morph weight targets")
                }
                util::MorphTargetWeights::I16(_weights) => {
                  generic_failure("i16", "morph weight targets")
                }
                util::MorphTargetWeights::U16(_weights) => {
                  generic_failure("u16", "morph weight targets")
                }
                util::MorphTargetWeights::F32(weights) => {
                  let mut container: Vec<f32> = vec![];

                  // There can be a bug in the iterator given due to how rust GLTF works, we want to drop out when the end is hit.
                  // This prevents an infinite loop.
                  let limit = weights.len();
                  for (index, value) in weights.enumerate() {
                    container.push(value);
                    // Bail out.
                    if index >= limit {
                      break;
                    }
                  }
                  Keyframes::Weights(container)
                }
              },
            };

            // And now we capture if this thing failed and stop it if it did.
            if blew_up {
              break;
            }

            keyframe_result
          } else {
            // * Something blew up, it's now a static model.
            error!(
                "minetest-gltf: Unknown keyframe in model [{}]. This model is probably corrupted. Model will not be animated.",
                file_name
              );
            bone_animation_channels.clear();
            break;
          };

          // ! THIS IS EXTREMELY WRONG !
          let bone_id = channel.target().node().index() as i32;

          println!("bone_id: {}", bone_id);

          let enable_debug_spam = false;

          if enable_debug_spam {
            println!("found target bone: {}", bone_id);
          }

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

            Keyframes::Explosion => {
              panic!("minetest-gltf: Explosion was somehow reached in animation!");
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

  bone_animation_channels
}

pub(crate) fn finalize_animations(
  minetest_gltf: &mut MinetestGLTF,
  gltf_data: Gltf,
  buffers: Vec<Data>,
  file_name: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
  // We're going to take the raw data.
  let bone_animations = grab_animations(gltf_data, buffers, file_name);

  // Then finalize it.
  // (finalization is interpolating the frames so they're all equal distance from eachother in the scale of time.)

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

  Ok(())
}
