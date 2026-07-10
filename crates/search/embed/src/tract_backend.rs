//! tract-onnx embedding backend (RFC-021).
//!
//! Only compiled when `--features tract` is passed. Loads an ONNX
//! model, runs mean-pooling over token embeddings, and L2-normalizes
//! the output.

use crate::EmbeddingModelConfig;
use orbok_core::{OrbokError, OrbokResult};
use orbok_models::{EmbeddingModel, l2_normalize};
use tokenizers::{
    Encoding, Tokenizer,
    utils::{
        padding::{PaddingDirection, PaddingParams, PaddingStrategy},
        truncation::TruncationParams,
    },
};
use tract_onnx::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InputKind {
    InputIds,
    AttentionMask,
    TokenTypeIds,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IntegerDatum {
    I32,
    I64,
}

#[derive(Clone, Debug)]
struct ModelInput {
    kind: InputKind,
    datum: IntegerDatum,
}

pub fn create(config: &EmbeddingModelConfig) -> OrbokResult<Box<dyn EmbeddingModel>> {
    if !std::path::Path::new(&config.weights_path).exists() {
        return Err(OrbokError::Cache(format!(
            "model weights not found: {}",
            config.weights_path
        )));
    }
    let Some(tokenizer_path) = &config.tokenizer_path else {
        return Err(OrbokError::Cache(
            "tokenizer file is required for ONNX inference".into(),
        ));
    };
    if !std::path::Path::new(tokenizer_path).exists() {
        return Err(OrbokError::Cache(format!(
            "tokenizer file not found: {tokenizer_path}"
        )));
    }
    let model = TractEmbeddingModel::load(config)?;
    Ok(Box::new(model))
}

struct TractEmbeddingModel {
    model: Arc<TypedSimplePlan>,
    tokenizer: Tokenizer,
    inputs: Vec<ModelInput>,
    dimension: u32,
    name: String,
    version: String,
}

impl TractEmbeddingModel {
    fn load(config: &EmbeddingModelConfig) -> OrbokResult<Self> {
        let tokenizer_path = config.tokenizer_path.as_ref().ok_or_else(|| {
            OrbokError::Cache("tokenizer file is required for ONNX inference".into())
        })?;
        let tokenizer = load_tokenizer(tokenizer_path, config.max_seq_len as usize)?;

        let model = tract_onnx::onnx()
            .model_for_path(&config.weights_path)
            .map_err(|e| OrbokError::Cache(format!("ONNX load failed: {e}")))?
            .into_optimized()
            .map_err(|e| OrbokError::Cache(format!("ONNX optimize failed: {e}")))?;
        let inputs = inspect_inputs(&model)?;
        let model = model
            .into_runnable()
            .map_err(|e| OrbokError::Cache(format!("ONNX runnable failed: {e}")))?;
        Ok(Self {
            model,
            tokenizer,
            inputs,
            dimension: config.dimension,
            name: config.model_name.clone(),
            version: config.model_version.clone(),
        })
    }
}

impl EmbeddingModel for TractEmbeddingModel {
    fn name(&self) -> &str {
        &self.name
    }
    fn version(&self) -> &str {
        &self.version
    }
    fn dimension(&self) -> u32 {
        self.dimension
    }

    fn embed_batch(&self, texts: &[&str]) -> OrbokResult<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let encodings = self
            .tokenizer
            .encode_batch(texts.to_vec(), true)
            .map_err(|e| OrbokError::Cache(format!("tokenization failed: {e}")))?;
        let batch = encodings.len();
        let seq_len = sequence_len(&encodings)?;
        let inputs = build_inputs(&self.inputs, &encodings, batch, seq_len)?;
        let outputs = self
            .model
            .run(inputs.into())
            .map_err(|e| OrbokError::Cache(format!("ONNX inference failed: {e}")))?;
        let output = outputs
            .into_iter()
            .next()
            .ok_or_else(|| OrbokError::Cache("ONNX inference returned no outputs".into()))?
            .into_tensor();
        vectors_from_output(&output, &encodings, batch, seq_len, self.dimension as usize)
    }
}

fn load_tokenizer(path: &str, max_seq_len: usize) -> OrbokResult<Tokenizer> {
    let mut tokenizer = Tokenizer::from_file(path)
        .map_err(|e| OrbokError::Cache(format!("tokenizer load failed: {e}")))?;
    tokenizer
        .with_truncation(Some(TruncationParams {
            max_length: max_seq_len,
            ..Default::default()
        }))
        .map_err(|e| OrbokError::Cache(format!("tokenizer truncation setup failed: {e}")))?;

    let (pad_token, pad_id) = pad_token(&tokenizer);
    tokenizer.with_padding(Some(PaddingParams {
        strategy: PaddingStrategy::Fixed(max_seq_len),
        direction: PaddingDirection::Right,
        pad_to_multiple_of: None,
        pad_id,
        pad_type_id: 0,
        pad_token,
    }));

    Ok(tokenizer)
}

fn pad_token(tokenizer: &Tokenizer) -> (String, u32) {
    ["<pad>", "[PAD]", "<PAD>", "[pad]"]
        .into_iter()
        .find_map(|token| {
            tokenizer
                .token_to_id(token)
                .map(|id| (token.to_string(), id))
        })
        .unwrap_or_else(|| ("[PAD]".to_string(), 0))
}

fn inspect_inputs(model: &TypedModel) -> OrbokResult<Vec<ModelInput>> {
    let mut inputs = Vec::new();
    for (index, outlet) in model
        .input_outlets()
        .map_err(|e| OrbokError::Cache(format!("ONNX input inspection failed: {e}")))?
        .iter()
        .enumerate()
    {
        let name = model
            .outlet_label(*outlet)
            .unwrap_or_else(|| model.node(outlet.node).name.as_str());
        let kind = input_kind(name)?;
        let datum = integer_datum(model.input_fact(index).map_err(|e| {
            OrbokError::Cache(format!("ONNX input fact inspection failed for {name}: {e}"))
        })?)?;
        inputs.push(ModelInput { kind, datum });
    }

    if !inputs.iter().any(|input| input.kind == InputKind::InputIds) {
        return Err(OrbokError::Cache(
            "ONNX model does not expose an input_ids input".into(),
        ));
    }

    Ok(inputs)
}

fn input_kind(name: &str) -> OrbokResult<InputKind> {
    let normalized = name.to_ascii_lowercase();
    if normalized.contains("input_ids") || normalized == "input" {
        Ok(InputKind::InputIds)
    } else if normalized.contains("attention_mask") {
        Ok(InputKind::AttentionMask)
    } else if normalized.contains("token_type_ids") {
        Ok(InputKind::TokenTypeIds)
    } else {
        Err(OrbokError::Cache(format!(
            "unsupported ONNX embedding input: {name}"
        )))
    }
}

fn integer_datum(fact: &TypedFact) -> OrbokResult<IntegerDatum> {
    if fact.datum_type == i64::datum_type() {
        Ok(IntegerDatum::I64)
    } else if fact.datum_type == i32::datum_type() {
        Ok(IntegerDatum::I32)
    } else {
        Err(OrbokError::Cache(format!(
            "unsupported ONNX embedding input datum type: {:?}",
            fact.datum_type
        )))
    }
}

fn sequence_len(encodings: &[Encoding]) -> OrbokResult<usize> {
    let Some(first) = encodings.first() else {
        return Ok(0);
    };
    let len = first.len();
    if encodings.iter().all(|encoding| encoding.len() == len) {
        Ok(len)
    } else {
        Err(OrbokError::Cache(
            "tokenizer produced uneven batch sequence lengths".into(),
        ))
    }
}

fn build_inputs(
    model_inputs: &[ModelInput],
    encodings: &[Encoding],
    batch: usize,
    seq_len: usize,
) -> OrbokResult<Vec<TValue>> {
    model_inputs
        .iter()
        .map(|input| {
            let values = encoding_values(input.kind, encodings);
            tensor_from_i64(input.datum, &[batch, seq_len], &values)
        })
        .collect()
}

fn encoding_values(kind: InputKind, encodings: &[Encoding]) -> Vec<i64> {
    encodings
        .iter()
        .flat_map(|encoding| {
            let values = match kind {
                InputKind::InputIds => encoding.get_ids(),
                InputKind::AttentionMask => encoding.get_attention_mask(),
                InputKind::TokenTypeIds => encoding.get_type_ids(),
            };
            values.iter().map(|value| i64::from(*value))
        })
        .collect()
}

fn tensor_from_i64(datum: IntegerDatum, shape: &[usize], values: &[i64]) -> OrbokResult<TValue> {
    match datum {
        IntegerDatum::I64 => Tensor::from_shape(shape, values)
            .map(IntoTValue::into_tvalue)
            .map_err(|e| OrbokError::Cache(format!("ONNX input tensor build failed: {e}"))),
        IntegerDatum::I32 => {
            let values: Vec<i32> = values
                .iter()
                .map(|value| i32::try_from(*value))
                .collect::<Result<_, _>>()
                .map_err(|e| OrbokError::Cache(format!("ONNX input id out of i32 range: {e}")))?;
            Tensor::from_shape(shape, &values)
                .map(IntoTValue::into_tvalue)
                .map_err(|e| OrbokError::Cache(format!("ONNX input tensor build failed: {e}")))
        }
    }
}

fn vectors_from_output(
    output: &Tensor,
    encodings: &[Encoding],
    batch: usize,
    seq_len: usize,
    dimension: usize,
) -> OrbokResult<Vec<Vec<f32>>> {
    let view = output
        .to_plain_array_view::<f32>()
        .map_err(|e| OrbokError::Cache(format!("ONNX output tensor read failed: {e}")))?;
    let shape = view.shape();

    let mut vectors = match shape {
        [actual_batch, actual_dim] if *actual_batch == batch => {
            if *actual_dim != dimension {
                return Err(dimension_mismatch(dimension, *actual_dim));
            }
            (0..batch)
                .map(|row| (0..dimension).map(|col| view[[row, col]]).collect())
                .collect()
        }
        [actual_batch, actual_seq_len, actual_dim]
            if *actual_batch == batch && *actual_seq_len == seq_len =>
        {
            if *actual_dim != dimension {
                return Err(dimension_mismatch(dimension, *actual_dim));
            }
            mean_pool_sequence_output(&view, encodings, batch, seq_len, dimension)
        }
        _ => {
            return Err(OrbokError::Cache(format!(
                "unsupported ONNX embedding output shape: {shape:?}"
            )));
        }
    };

    for vector in &mut vectors {
        l2_normalize(vector);
    }
    Ok(vectors)
}

fn mean_pool_sequence_output(
    view: &tract_ndarray::ArrayViewD<'_, f32>,
    encodings: &[Encoding],
    batch: usize,
    seq_len: usize,
    dimension: usize,
) -> Vec<Vec<f32>> {
    let mut vectors = Vec::with_capacity(batch);
    for batch_index in 0..batch {
        let mut vector = vec![0.0; dimension];
        let mut token_count = 0.0f32;
        for token_index in 0..seq_len {
            if encodings[batch_index].get_attention_mask()[token_index] == 0 {
                continue;
            }
            token_count += 1.0;
            for dim_index in 0..dimension {
                vector[dim_index] += view[[batch_index, token_index, dim_index]];
            }
        }
        if token_count > 0.0 {
            for value in &mut vector {
                *value /= token_count;
            }
        }
        vectors.push(vector);
    }
    vectors
}

fn dimension_mismatch(expected: usize, actual: usize) -> OrbokError {
    OrbokError::Cache(format!(
        "ONNX output dimension mismatch: expected {expected}, got {actual}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_kind_accepts_standard_transformer_names() {
        assert_eq!(input_kind("input_ids").unwrap(), InputKind::InputIds);
        assert_eq!(
            input_kind("attention_mask").unwrap(),
            InputKind::AttentionMask
        );
        assert_eq!(
            input_kind("token_type_ids").unwrap(),
            InputKind::TokenTypeIds
        );
    }

    #[test]
    fn input_kind_rejects_unknown_names() {
        let err = input_kind("position_ids").unwrap_err().to_string();
        assert!(err.contains("unsupported ONNX embedding input"));
    }

    #[test]
    fn tensor_from_i64_supports_i64_and_i32_model_inputs() {
        let i64_value = tensor_from_i64(IntegerDatum::I64, &[1, 2], &[1, 2]).unwrap();
        assert_eq!(i64_value.datum_type(), i64::datum_type());

        let i32_value = tensor_from_i64(IntegerDatum::I32, &[1, 2], &[1, 2]).unwrap();
        assert_eq!(i32_value.datum_type(), i32::datum_type());
    }

    #[test]
    fn create_requires_tokenizer_path_before_loading_onnx() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        let config = EmbeddingModelConfig {
            weights_path: temp.path().to_string_lossy().into_owned(),
            tokenizer_path: None,
            dimension: 384,
            max_seq_len: 512,
            backend: orbok_models::InferenceBackend::OnnxRuntime,
            model_name: "test".into(),
            model_version: "v1".into(),
        };

        match create(&config) {
            Err(err) => assert!(err.to_string().contains("tokenizer file is required")),
            Ok(_) => panic!("ONNX backend should require a tokenizer path"),
        }
    }
}
