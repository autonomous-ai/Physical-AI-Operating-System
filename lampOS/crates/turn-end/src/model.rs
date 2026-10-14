//! Pinned Smart Turn v3.2 ONNX model executed with tract.
use crate::{FRAMES, MELS, Result};
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::Path, sync::Arc};
use tract_onnx::prelude::*;

/// A model file pinned by size and SHA256. It is fetched separately and never
/// committed; docs/endpoint-detection.md gives the download and the
/// `lamp-turn-end --check-model` verification.
#[derive(Clone, Copy, Debug)]
pub struct ModelAsset {
    pub file_name: &'static str,
    pub bytes: u64,
    pub sha256: &'static str,
    pub source: &'static str,
}

/// The unquantized v3.2 export. The int8 `smart-turn-v3.2-cpu.onnx` that
/// Pipecat bundles is not used: tract 0.23.8 mis-executes its uint8
/// QuantizeLinear/DequantizeLinear pairs (softmax outputs dequantize to
/// 255 x scale), while this export matches ONNX Runtime within 1e-6.
pub const SMART_TURN_V3_2: ModelAsset = ModelAsset {
    file_name: lamp_turn_end_wire::SMART_TURN_MODEL_FILE,
    bytes: 32_411_198,
    sha256: lamp_turn_end_wire::SMART_TURN_MODEL_SHA256,
    source: "https://huggingface.co/pipecat-ai/smart-turn-v3/resolve/f766f81d3cfdf7737ac64aad813d91bbfd56bf93/smart-turn-v3.2-gpu.onnx",
};

/// Reads a pinned file whole and returns its bytes only if their size and
/// SHA256 are the pinned ones.
pub fn read_pinned(path: &Path, bytes: u64, sha256: &str) -> Result<Vec<u8>> {
    let file = File::open(path)?;
    if file.metadata()?.len() != bytes {
        return Err(format!("{} has an unexpected size", path.display()).into());
    }
    let mut contents = Vec::with_capacity(bytes as usize);
    file.take(bytes + 1).read_to_end(&mut contents)?;
    if contents.len() as u64 != bytes {
        return Err(format!("{} changed while it was read", path.display()).into());
    }
    let digest = Sha256::digest(&contents)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if digest != sha256 {
        return Err(format!("{} does not match the pinned SHA256", path.display()).into());
    }
    Ok(contents)
}

pub struct SmartTurn {
    plan: Arc<TypedRunnableModel>,
}

impl SmartTurn {
    /// Verifies the exact pinned bytes before the ONNX parser sees them.
    pub fn load(path: &Path, asset: &ModelAsset) -> Result<Self> {
        let bytes = read_pinned(path, asset.bytes, asset.sha256)?;
        // The export annotates a symbolic batch that conflicts with its
        // fixed-batch reshapes; tract re-infers every shape from one batch.
        let plan = tract_onnx::onnx()
            .with_ignore_value_info(true)
            .with_ignore_output_shapes(true)
            .model_for_read(&mut bytes.as_slice())?
            .with_input_fact(0, f32::fact([1, MELS, FRAMES]).into())?
            .into_optimized()?
            .into_runnable()?;
        Ok(Self { plan })
    }

    /// Probability that the analyzed speech is a finished turn.
    pub fn probability(&self, features: Vec<f32>) -> Result<f32> {
        if features.len() != MELS * FRAMES || features.iter().any(|value| !value.is_finite()) {
            return Err("invalid Smart Turn features".into());
        }
        let input = Tensor::from_shape(&[1, MELS, FRAMES], &features)?;
        let output = self.plan.run(tvec!(input.into()))?;
        let values = output
            .first()
            .ok_or("Smart Turn returned no output")?
            .to_plain_array_view::<f32>()?;
        let mut values = values.iter();
        match (values.next(), values.next()) {
            (Some(&probability), None) if (0.0..=1.0).contains(&probability) => Ok(probability),
            _ => Err("Smart Turn returned an invalid probability".into()),
        }
    }
}
