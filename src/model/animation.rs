// Based on https://whoisryosuke.com/blog/2022/importing-gltf-with-wgpu-and-rust
// You can thank ryosuke for this information.

use ahash::AHashMap;
use glam::{Quat, Vec3};
use gltf::{animation::util, buffer::Data, Gltf};
use log::error;

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
  pub fn new() -> Self {
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
/// This cleans up the implementation when parsing the GLTF morph target weights.
///
/// It converts &[T] into a Vec<f32> which is the Keyframes::Weights enum.
///
// macro_rules! weightify {
//   ($x:expr) => {{

//   }};
// }

pub fn grab_animations(
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
