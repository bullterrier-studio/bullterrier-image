//! SAM 2 on ONNX Runtime: the encoder turns the photo into embeddings, the decoder turns the embeddings and the prompts into
//! candidate masks ([`finder`]).

use std::sync::Mutex;

use ort::session::Session as OrtSession;
use ort::value::Tensor as OrtTensor;

use super::onnx::{Options, model_file, open_session};
use super::prompted::{Answer, PromptedFinder, Prompts, Query, Segment};
use super::{SAM2_DECODER, SAM2_ENCODER, Tensor};
use crate::Error;

/// The encoder's outputs, which the decoder takes under the same names: the image embeddings at three scales.
const EMBEDDINGS: [&str; 3] = ["image_embeddings.0", "image_embeddings.1", "image_embeddings.2"];

/// The two halves of SAM 2, loaded.
struct Sam2 {
    encoder: Mutex<OrtSession>,
    decoder: Mutex<OrtSession>,
}

/// A [`PromptedFinder`] for `prompts` with SAM 2 run by ONNX Runtime: finds the two model files in the cache folder
/// (downloading them there if `options` allow), finds the library and loads all.
///
/// # Errors
/// [`Error::BackendUnavailable`] as [`onnx::finder`](super::onnx::finder) says, and for a local model file in `options`: SAM 2
/// is two files, kept in the cache folder only.
pub fn finder(options: &Options, prompts: Prompts) -> Result<PromptedFinder, Error> {
    if let Some(local) = options.model_file() {
        return Err(Error::BackendUnavailable(format!(
            "SAM 2 is two model files and takes no local copy ({}): keep them in the cache folder",
            local.display()
        )));
    }
    let encoder = model_file(&SAM2_ENCODER, options)?;
    let decoder = model_file(&SAM2_DECODER, options)?;
    let library = super::runtime::library(options, &options.cache_folder()?)?;
    let session = Sam2 {
        encoder: Mutex::new(open_session(&library, &encoder)?),
        decoder: Mutex::new(open_session(&library, &decoder)?),
    };
    Ok(PromptedFinder::new(prompts, session))
}

impl Segment for Sam2 {
    fn segment(&self, query: &Query) -> Result<Answer, Error> {
        let unavailable = |error: &dyn std::fmt::Display| {
            Error::BackendUnavailable(format!("onnxruntime could not run SAM 2 on this input: {error}"))
        };
        let poisoned = || Error::BackendUnavailable("a model run panicked earlier".to_owned());
        let tensor = |shape: Vec<usize>, values: Vec<f32>| {
            let shape: Vec<i64> = shape
                .into_iter()
                .map(|side| i64::try_from(side).unwrap_or(i64::MAX))
                .collect();
            OrtTensor::from_array((shape, values)).map_err(|error| unavailable(&error))
        };
        let mut embeddings = Vec::with_capacity(EMBEDDINGS.len());
        {
            let mut encoder = self.encoder.lock().map_err(|_| poisoned())?;
            let outputs = encoder
                .run(ort::inputs!["pixel_values" => tensor(query.image.shape().to_vec(), query.image.values().to_vec())?])
                .map_err(|error| unavailable(&error))?;
            for name in EMBEDDINGS {
                let (shape, values) = outputs[name]
                    .try_extract_tensor::<f32>()
                    .map_err(|error| unavailable(&error))?;
                embeddings.push((name, shape.to_vec(), values.to_vec()));
            }
        }
        let clicks = query.points.len();
        let boxes = query.boxes.len();
        let mut inputs: Vec<(String, ort::value::DynValue)> = Vec::new();
        for (name, shape, values) in embeddings {
            let shape = shape
                .into_iter()
                .map(|side| usize::try_from(side).unwrap_or(0))
                .collect();
            inputs.push((name.to_owned(), tensor(shape, values)?.into_dyn()));
        }
        let points: Vec<f32> = query.points.iter().flatten().copied().collect();
        inputs.push((
            "input_points".to_owned(),
            tensor(vec![1, 1, clicks, 2], points)?.into_dyn(),
        ));
        let labels = OrtTensor::from_array((
            vec![1_i64, 1, i64::try_from(clicks).unwrap_or(i64::MAX)],
            query.labels.clone(),
        ))
        .map_err(|error| unavailable(&error))?;
        inputs.push(("input_labels".to_owned(), labels.into_dyn()));
        let corners: Vec<f32> = query.boxes.iter().flatten().copied().collect();
        inputs.push(("input_boxes".to_owned(), tensor(vec![1, boxes, 4], corners)?.into_dyn()));
        let mut decoder = self.decoder.lock().map_err(|_| poisoned())?;
        let outputs = decoder.run(inputs).map_err(|error| unavailable(&error))?;
        let (_, scores) = outputs["iou_scores"]
            .try_extract_tensor::<f32>()
            .map_err(|error| unavailable(&error))?;
        let (shape, logits) = outputs["pred_masks"]
            .try_extract_tensor::<f32>()
            .map_err(|error| unavailable(&error))?;
        // pred_masks is 1 x 1 x candidates x rows x columns
        let shape: Vec<usize> = shape
            .iter()
            .skip(2)
            .map(|&side| usize::try_from(side).unwrap_or(0))
            .collect();
        Ok(Answer {
            scores: scores.to_vec(),
            logits: Tensor::new(shape, logits.to_vec())?,
        })
    }
}
