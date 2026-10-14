//! The pinned model with its weight multiplications done in 8 bits.
//!
//! Each `MatMul` whose second input is a stored weight matrix becomes the
//! form ONNX Runtime's dynamic quantization writes, and which ONNX Runtime
//! runs as one 8-bit multiplication:
//!
//! - the weight matrix, `[K, N]` floats, is stored as int8 with one scale per
//!   output column and a zero offset (symmetric, `scale = max |w| / 127.5`,
//!   values rounded half to even and clamped to `[-128, 127]`, as
//!   `onnxruntime.quantization.quantize_dynamic` does with `per_channel`);
//! - the activation is quantized as it arrives (`DynamicQuantizeLinear`:
//!   uint8, its own scale and zero point, once per activation even when
//!   several multiplications share it);
//! - `MatMulInteger`, `Cast` to float and one multiplication by the product
//!   of the two scales give back the original output.
//!
//! Everything else, the attention products, the two input convolutions, the
//! normalisations and the classifier head among them, stays in 32-bit floats,
//! and so does any multiplication the caller names in `keep`.
//! The rewrite is done in memory each time the worker starts, from the pinned
//! float model, so no second model file exists.
use crate::Result;
use std::collections::{HashMap, HashSet};
use tract_onnx::pb::{
    AttributeProto, GraphProto, ModelProto, NodeProto, TensorProto, tensor_proto::DataType,
};

/// Weight matrices with fewer rows or columns than this stay in floats: the
/// saving is nil and the rounding is relatively larger.
const MIN_SIDE: usize = 16;

/// What the rewrite changed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rewrite {
    /// Weight multiplications now done in 8 bits.
    pub multiplications: usize,
    /// Activations quantized on arrival (shared by the multiplications that
    /// read the same activation).
    pub activations: usize,
}

/// Rewrites every eligible weight multiplication of `model` in place, except
/// the nodes named in `keep`, each of which must exist.
pub fn rewrite(model: &mut ModelProto, keep: &[&str]) -> Result<Rewrite> {
    let graph = model.graph.as_mut().ok_or("the model has no graph")?;
    for name in keep {
        if !graph.node.iter().any(|node| node.name == *name) {
            return Err(format!("the model has no node {name} to keep in floats").into());
        }
    }
    let weights: HashMap<String, usize> = graph
        .initializer
        .iter()
        .enumerate()
        .map(|(index, tensor)| (tensor.name.clone(), index))
        .collect();
    let mut names = Names::new(graph);
    let mut quantized: HashMap<String, [String; 3]> = HashMap::new();
    let mut added = Vec::new();
    let mut nodes = Vec::with_capacity(graph.node.len() * 2);
    let mut summary = Rewrite::default();
    for node in std::mem::take(&mut graph.node) {
        let eligible = node.op_type == "MatMul"
            && node.domain.is_empty()
            && node.input.len() == 2
            && !keep.contains(&node.name.as_str());
        let matrix = eligible
            .then(|| weights.get(&node.input[1]))
            .flatten()
            .map(|&index| float_matrix(&graph.initializer[index]))
            .transpose()?
            .flatten();
        let Some((rows, columns, values)) = matrix else {
            nodes.push(node);
            continue;
        };
        let activation = node.input[0].clone();
        let [input, input_scale, input_zero] = match quantized.get(&activation) {
            Some(outputs) => outputs.clone(),
            None => {
                let outputs = [
                    names.fresh(&format!("{activation}_eight_bit")),
                    names.fresh(&format!("{activation}_eight_bit_scale")),
                    names.fresh(&format!("{activation}_eight_bit_zero")),
                ];
                nodes.push(op(
                    names.fresh(&format!("{activation}_eight_bit_quantize")),
                    "DynamicQuantizeLinear",
                    vec![activation.clone()],
                    outputs.to_vec(),
                    vec![],
                ));
                quantized.insert(activation, outputs.clone());
                summary.activations += 1;
                outputs
            }
        };
        let weight = &node.input[1];
        let (levels, scales) = per_column(rows, columns, &values);
        let weight_levels = names.fresh(&format!("{weight}_eight_bit"));
        let weight_scales = names.fresh(&format!("{weight}_eight_bit_scale"));
        let weight_zero = names.fresh(&format!("{weight}_eight_bit_zero"));
        added.push(int8_tensor(
            &weight_levels,
            &[rows as i64, columns as i64],
            &levels,
        ));
        added.push(float_tensor(&weight_scales, &[columns as i64], &scales));
        added.push(int8_tensor(
            &weight_zero,
            &[columns as i64],
            &vec![0; columns],
        ));
        let output = &node.output[0];
        let scale = names.fresh(&format!("{output}_eight_bit_scale"));
        let integer = names.fresh(&format!("{output}_eight_bit_sum"));
        let float = names.fresh(&format!("{output}_eight_bit_float"));
        nodes.push(op(
            names.fresh(&format!("{output}_eight_bit_scales")),
            "Mul",
            vec![input_scale, weight_scales],
            vec![scale.clone()],
            vec![],
        ));
        nodes.push(op(
            names.fresh(&format!("{output}_eight_bit_multiply")),
            "MatMulInteger",
            vec![input, weight_levels, input_zero, weight_zero],
            vec![integer.clone()],
            vec![],
        ));
        nodes.push(op(
            names.fresh(&format!("{output}_eight_bit_cast")),
            "Cast",
            vec![integer],
            vec![float.clone()],
            vec![AttributeProto {
                name: "to".into(),
                r#type: tract_onnx::pb::attribute_proto::AttributeType::Int as i32,
                i: DataType::Float as i64,
                ..Default::default()
            }],
        ));
        nodes.push(op(
            names.fresh(&format!("{output}_eight_bit_rescale")),
            "Mul",
            vec![float, scale],
            vec![output.clone()],
            vec![],
        ));
        summary.multiplications += 1;
    }
    graph.node = nodes;
    // The float copies of the rewritten weights are no longer read.
    let read: HashSet<&str> = graph
        .node
        .iter()
        .flat_map(|node| node.input.iter().map(String::as_str))
        .chain(graph.output.iter().map(|output| output.name.as_str()))
        .collect();
    let kept = std::mem::take(&mut graph.initializer)
        .into_iter()
        .filter(|tensor| read.contains(tensor.name.as_str()))
        .collect::<Vec<_>>();
    graph.initializer = kept;
    graph.initializer.extend(added);
    Ok(summary)
}

/// Symmetric int8 levels and one scale per column of a row-major `[rows,
/// columns]` matrix, as ONNX Runtime's `quantize_dynamic` computes them.
pub fn per_column(rows: usize, columns: usize, values: &[f32]) -> (Vec<i8>, Vec<f32>) {
    let mut scales = vec![0f32; columns];
    for row in values.chunks_exact(columns).take(rows) {
        for (largest, value) in scales.iter_mut().zip(row) {
            *largest = largest.max(value.abs());
        }
    }
    for scale in &mut scales {
        *scale = if *scale > 0.0 {
            2.0 * *scale / 255.0
        } else {
            1.0
        };
    }
    let levels = values
        .chunks_exact(columns)
        .flat_map(|row| {
            row.iter()
                .zip(&scales)
                .map(|(value, scale)| (value / scale).round_ties_even().clamp(-128.0, 127.0) as i8)
        })
        .collect();
    (levels, scales)
}

/// A stored float matrix with both sides at least [`MIN_SIDE`], as rows,
/// columns and row-major values. `None` for anything else.
fn float_matrix(tensor: &TensorProto) -> Result<Option<(usize, usize, Vec<f32>)>> {
    let [rows, columns] = tensor.dims[..] else {
        return Ok(None);
    };
    if tensor.data_type != DataType::Float as i32 || tensor.data_location == Some(1) {
        return Ok(None);
    }
    let (rows, columns) = (usize::try_from(rows)?, usize::try_from(columns)?);
    if rows < MIN_SIDE || columns < MIN_SIDE {
        return Ok(None);
    }
    let values = if tensor.raw_data.is_empty() {
        tensor.float_data.clone()
    } else {
        tensor
            .raw_data
            .chunks_exact(4)
            .map(|bytes| f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
            .collect()
    };
    if values.len() != rows * columns {
        return Err(format!("weight {} holds {} values", tensor.name, values.len()).into());
    }
    Ok(Some((rows, columns, values)))
}

fn op(
    name: String,
    op_type: &str,
    input: Vec<String>,
    output: Vec<String>,
    attribute: Vec<AttributeProto>,
) -> NodeProto {
    NodeProto {
        name,
        op_type: op_type.into(),
        input,
        output,
        attribute,
        ..Default::default()
    }
}

fn int8_tensor(name: &str, dims: &[i64], values: &[i8]) -> TensorProto {
    TensorProto {
        name: name.into(),
        dims: dims.to_vec(),
        data_type: DataType::Int8 as i32,
        raw_data: values.iter().map(|&value| value as u8).collect(),
        ..Default::default()
    }
}

fn float_tensor(name: &str, dims: &[i64], values: &[f32]) -> TensorProto {
    TensorProto {
        name: name.into(),
        dims: dims.to_vec(),
        data_type: DataType::Float as i32,
        raw_data: values
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect(),
        ..Default::default()
    }
}

/// Names not yet used by any node, value or stored tensor of the graph.
struct Names(HashSet<String>);

impl Names {
    fn new(graph: &GraphProto) -> Self {
        let mut taken = HashSet::new();
        for node in &graph.node {
            taken.insert(node.name.clone());
            taken.extend(node.input.iter().cloned());
            taken.extend(node.output.iter().cloned());
        }
        taken.extend(graph.initializer.iter().map(|tensor| tensor.name.clone()));
        taken.extend(graph.input.iter().map(|value| value.name.clone()));
        taken.extend(graph.output.iter().map(|value| value.name.clone()));
        Self(taken)
    }

    fn fresh(&mut self, base: &str) -> String {
        let mut name = base.to_owned();
        let mut suffix = 1;
        while !self.0.insert(name.clone()) {
            suffix += 1;
            name = format!("{base}_{suffix}");
        }
        name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tract_onnx::pb::{
        OperatorSetIdProto, TensorShapeProto, TypeProto, ValueInfoProto, tensor_shape_proto,
        type_proto,
    };
    use tract_onnx::prelude::*;

    fn value(name: &str, dims: &[i64]) -> ValueInfoProto {
        ValueInfoProto {
            name: name.into(),
            r#type: Some(TypeProto {
                value: Some(type_proto::Value::TensorType(type_proto::Tensor {
                    elem_type: DataType::Float as i32,
                    shape: Some(TensorShapeProto {
                        dim: dims
                            .iter()
                            .map(|&d| tensor_shape_proto::Dimension {
                                value: Some(tensor_shape_proto::dimension::Value::DimValue(d)),
                                ..Default::default()
                            })
                            .collect(),
                    }),
                })),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    fn pseudo(seed: u32, count: usize, size: f32) -> Vec<f32> {
        let mut state = seed;
        (0..count)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                ((state % 20_001) as f32 / 10_000.0 - 1.0) * size
            })
            .collect()
    }

    /// `x [4, 32]` times two weights that share `x`, then a third weight on
    /// their sum, and a small matrix that must stay in floats.
    fn model() -> ModelProto {
        let first = float_tensor("first", &[32, 24], &pseudo(1, 32 * 24, 0.3));
        let second = float_tensor("second", &[32, 24], &pseudo(2, 32 * 24, 0.2));
        let third = float_tensor("third", &[24, 16], &pseudo(3, 24 * 16, 0.5));
        let small = float_tensor("small", &[16, 4], &pseudo(4, 16 * 4, 0.5));
        let node = |name: &str, op_type: &str, input: &[&str], output: &str| NodeProto {
            name: name.into(),
            op_type: op_type.into(),
            input: input.iter().map(|&s| s.into()).collect(),
            output: vec![output.into()],
            ..Default::default()
        };
        ModelProto {
            ir_version: 8,
            opset_import: vec![OperatorSetIdProto {
                domain: String::new(),
                version: 18,
            }],
            graph: Some(GraphProto {
                name: "test".into(),
                node: vec![
                    node("a", "MatMul", &["x", "first"], "y1"),
                    node("b", "MatMul", &["x", "second"], "y2"),
                    node("c", "Add", &["y1", "y2"], "y3"),
                    node("d", "MatMul", &["y3", "third"], "y4"),
                    node("e", "MatMul", &["y4", "small"], "y"),
                ],
                initializer: vec![first, second, third, small],
                input: vec![value("x", &[4, 32])],
                output: vec![value("y", &[4, 4])],
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    fn run(model: &ModelProto, input: &[f32]) -> Vec<f32> {
        let plan = tract_onnx::onnx()
            .model_for_proto_model(model)
            .unwrap()
            .into_optimized()
            .unwrap()
            .into_runnable()
            .unwrap();
        let input = Tensor::from_shape(&[4, 32], input).unwrap();
        let output = plan.run(tvec!(input.into())).unwrap();
        output[0]
            .to_plain_array_view::<f32>()
            .unwrap()
            .iter()
            .copied()
            .collect()
    }

    #[test]
    fn per_column_matches_onnx_runtime_conventions() {
        // Column scales are 2 max|w| / 255; an all-zero column gets 1.
        let values = [1.0, -0.5, 0.0, -2.0, 0.25, 0.0, 0.5, 2.0, 0.0];
        let (levels, scales) = per_column(3, 3, &values);
        assert_eq!(scales, [4.0 / 255.0, 4.0 / 255.0, 1.0]);
        // In f32, as ONNX Runtime's tool divides: 1 / (4/255) = 63.75 -> 64,
        // and +-2 / (4/255) = +-127.49999 -> +-127, inside the clamp.
        assert_eq!(levels, [64, -32, 0, -127, 16, 0, 32, 127, 0]);
        // Rounding is half to even, and the clamp holds the extremes.
        let (levels, _) = per_column(16, 1, &[2.5 * 4.0 / 255.0; 16]);
        assert_eq!(levels, [127; 16]);
        let (levels, _) = per_column(2, 1, &[-1.0, 1.0 / 127.5 * 0.5]);
        assert_eq!(levels, [-127, 0]);
    }

    #[test]
    fn shared_activations_are_quantized_once_and_small_matrices_stay() {
        let mut rewritten = model();
        let summary = rewrite(&mut rewritten, &[]).unwrap();
        assert_eq!(
            summary,
            Rewrite {
                multiplications: 3,
                activations: 2
            }
        );
        let graph = rewritten.graph.unwrap();
        let count = |op: &str| graph.node.iter().filter(|n| n.op_type == op).count();
        assert_eq!(count("DynamicQuantizeLinear"), 2);
        assert_eq!(count("MatMulInteger"), 3);
        // Only the 16 x 4 matrix is still multiplied in floats.
        assert_eq!(count("MatMul"), 1);
        let names: HashSet<_> = graph.initializer.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains("small"));
        for gone in ["first", "second", "third"] {
            assert!(!names.contains(gone), "{gone} float copy kept");
        }
        // Every value a node reads is produced earlier, stored or an input.
        let mut known: HashSet<String> = names.iter().map(|n| n.to_string()).collect();
        known.insert("x".into());
        for node in &graph.node {
            for input in &node.input {
                assert!(known.contains(input), "{} reads {input} early", node.name);
            }
            known.extend(node.output.iter().cloned());
        }
    }

    #[test]
    fn the_rewritten_model_gives_the_float_answer_within_eight_bit_rounding() {
        let input = pseudo(9, 4 * 32, 1.0);
        let expected = run(&model(), &input);
        let mut rewritten = model();
        rewrite(&mut rewritten, &[]).unwrap();
        let actual = run(&rewritten, &input);
        let largest = expected.iter().fold(0f32, |m, v| m.max(v.abs()));
        let worst = expected
            .iter()
            .zip(&actual)
            .fold(0f32, |m, (e, a)| m.max((e - a).abs()));
        // Three 8-bit stages: a few percent of the output's range at most.
        assert!(worst < 0.03 * largest, "worst {worst} of {largest}");
        assert!(worst > 0.0, "nothing was rounded");
    }

    #[test]
    fn named_multiplications_stay_in_floats_and_unknown_names_are_refused() {
        let mut kept = model();
        let summary = rewrite(&mut kept, &["d"]).unwrap();
        assert_eq!(
            summary,
            Rewrite {
                multiplications: 2,
                activations: 1
            }
        );
        let graph = kept.graph.unwrap();
        let d = graph.node.iter().find(|n| n.name == "d").unwrap();
        assert_eq!(
            (d.op_type.as_str(), d.input[1].as_str()),
            ("MatMul", "third")
        );
        assert!(graph.initializer.iter().any(|t| t.name == "third"));
        let error = rewrite(&mut model(), &["nowhere"]).unwrap_err().to_string();
        assert!(error.contains("nowhere"), "{error}");
    }

    #[test]
    fn names_never_collide_with_existing_ones() {
        let mut model = model();
        let graph = model.graph.as_mut().unwrap();
        // A value already named like the rewrite's first choice.
        graph.node[2].output[0] = "x_eight_bit".into();
        graph.node[3].input[0] = "x_eight_bit".into();
        rewrite(&mut model, &[]).unwrap();
        let graph = model.graph.unwrap();
        let mut produced = HashSet::new();
        for node in &graph.node {
            for output in &node.output {
                assert!(produced.insert(output.clone()), "{output} produced twice");
            }
        }
    }
}
