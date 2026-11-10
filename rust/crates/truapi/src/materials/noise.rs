//! The noise image a body's materials read as `r_noise`, made the same way by
//! every renderer from a seed the body's own id gives.

use sp_crypto_hashing::blake2_256;
use truapi::latest::RenderContext;

/// Bytes in the 1024 × 1024 noise image.
const NOISE_BYTES: u32 = 1024 * 1024;

/// Bytes one hash gives.
const CHUNK_BYTES: u32 = 32;

/// The seed of the body `context` names: the 32-bit FNV-1a hash of its id.
pub fn noise_seed(context: &RenderContext) -> u32 {
	let id = match context {
		RenderContext::ChatMessage { message_id, .. } => message_id,
		RenderContext::InputWidget { candidate_id } => candidate_id,
		RenderContext::PocketCard { card_id } => card_id,
	};
	id.bytes()
		.fold(0x811c_9dc5, |hash, byte| (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193))
}

/// The noise image of `seed`: one byte per pixel, row after row.
pub fn noise_image(seed: u32) -> Vec<u8> {
	(0..NOISE_BYTES / CHUNK_BYTES)
		.flat_map(|chunk| noise_chunk(seed, chunk))
		.collect()
}

/// Chunk `chunk` of the image: BLAKE2b-256 of the seed then the chunk index,
/// both little-endian.
fn noise_chunk(seed: u32, chunk: u32) -> [u8; 32] {
	blake2_256(&[seed.to_le_bytes(), chunk.to_le_bytes()].concat())
}

#[cfg(test)]
mod tests;
