//! The static embedding table, and the mean that is the whole of inference.
//!
//! There is no inference runtime here and no C++ behind one: a Model2Vec-class
//! model is a lookup table and an average (ADR-056). The rows are read in
//! place, out of the very buffer whose SHA-256 became the artifact identity,
//! so nothing is converted, copied or re-derived between the hash and the
//! arithmetic.

use crate::artifact::ArtifactError;
use safetensors::tensor::{Dtype, SafeTensors};

/// The names the two publishers of this model class use for the one tensor.
const TENSOR_NAMES: [&str; 3] = ["embeddings", "embedding.weight", "0"];

pub struct EmbeddingTable {
    /// The whole artifact buffer. Kept rather than copied out of: a second
    /// `Vec<f32>` of the same table costs another 489 MiB of resident memory
    /// and half a second of load time, both measured.
    bytes: Vec<u8>,
    /// Where the tensor's payload starts inside `bytes`.
    offset: usize,
    rows: usize,
    dimension: usize,
}

impl EmbeddingTable {
    pub fn parse(
        bytes: Vec<u8>,
        expected_rows: usize,
        expected_dimension: usize,
    ) -> Result<Self, ArtifactError> {
        let (offset, rows, dimension) = {
            let tensors = SafeTensors::deserialize(&bytes).map_err(|_| ArtifactError::Malformed)?;
            let tensor = TENSOR_NAMES
                .iter()
                .find_map(|name| tensors.tensor(name).ok())
                .ok_or(ArtifactError::Malformed)?;
            if tensor.dtype() != Dtype::F32 {
                return Err(ArtifactError::Malformed);
            }
            let shape = tensor.shape();
            if shape.len() != 2 {
                return Err(ArtifactError::Malformed);
            }
            let (rows, dimension) = (shape[0], shape[1]);
            if rows == 0 || dimension == 0 {
                return Err(ArtifactError::Malformed);
            }
            let payload = tensor.data();
            let expected_len = rows
                .checked_mul(dimension)
                .and_then(|cells| cells.checked_mul(4))
                .ok_or(ArtifactError::Malformed)?;
            if payload.len() != expected_len || payload.len() > bytes.len() {
                return Err(ArtifactError::Malformed);
            }
            // The payload is a window into `bytes`; everything before it is the
            // eight-byte header length plus the header itself.
            (bytes.len() - payload.len(), rows, dimension)
        };

        if rows != expected_rows || dimension != expected_dimension {
            return Err(ArtifactError::Unexpected);
        }

        Ok(Self {
            bytes,
            offset,
            rows,
            dimension,
        })
    }

    pub fn dimension(&self) -> usize {
        self.dimension
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    /// The mean of the rows named by `ids`, L2-normalised.
    ///
    /// Returns `None` when no identifier named a row — an input with nothing in
    /// the table has no place in the space, and the honest answer is the
    /// absence of a vector rather than the origin, which has no direction and
    /// therefore no cosine with anything.
    pub fn pool(&self, ids: &[u32]) -> Option<Vec<f32>> {
        let mut sum = vec![0.0f32; self.dimension];
        let mut count = 0usize;
        for id in ids {
            let row = *id as usize;
            if row >= self.rows {
                // A token identifier outside the table is a tokenizer and a
                // table that do not belong together. Skipped rather than
                // trusted; if every identifier is skipped the answer is `None`.
                continue;
            }
            let start = self.offset + row * self.dimension * 4;
            let window = &self.bytes[start..start + self.dimension * 4];
            for (index, slot) in sum.iter_mut().enumerate() {
                let at = index * 4;
                *slot += f32::from_le_bytes([
                    window[at],
                    window[at + 1],
                    window[at + 2],
                    window[at + 3],
                ]);
            }
            count += 1;
        }
        if count == 0 {
            return None;
        }
        let denominator = count as f32;
        for value in &mut sum {
            *value /= denominator;
        }
        let norm = sum.iter().map(|value| value * value).sum::<f32>().sqrt();
        if !norm.is_finite() || norm <= 0.0 {
            // A table of zeros, or one whose rows cancel exactly. Neither is a
            // direction, and `EmbeddingVector` would refuse it a step later
            // anyway; refusing here keeps the reason precise.
            return None;
        }
        for value in &mut sum {
            *value /= norm;
        }
        Some(sum)
    }
}
