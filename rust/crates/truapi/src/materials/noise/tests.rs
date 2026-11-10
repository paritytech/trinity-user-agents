use serde::Deserialize;
use sha2::{Digest, Sha256};
use truapi::latest::RenderContext;

use super::*;

/// Shaderlab's noise vectors, from shaderlab-spa at `f8251d2b`.
const NOISE: &str = include_str!("../../../tests/fixtures/materials/noise.v1.json");

/// Shaderlab's seed vectors, from shaderlab-spa PR #97 at `4fb610d1`.
const SEEDS: &str = include_str!("../../../tests/fixtures/materials/body-seed.v1.json");

#[derive(Deserialize)]
struct NoiseVectors {
	chunks: Vec<ChunkCase>,
	fields: Vec<FieldCase>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChunkCase {
	seed: u32,
	chunk: u32,
	expect_hex: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FieldCase {
	seed: u32,
	expect_sha256: String,
}

#[derive(Deserialize)]
struct SeedVectors {
	cases: Vec<SeedCase>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SeedCase {
	id: String,
	expect_seed: u32,
	expect_chunk0_hex: String,
}

/// A material that reads noise looks the same in every renderer only if
/// every renderer makes the same noise.
#[test]
fn the_noise_image_matches_the_vectors() {
	let vectors: NoiseVectors = serde_json::from_str(NOISE).unwrap();

	let chunks: Vec<_> = vectors
		.chunks
		.iter()
		.map(|case| hex::encode(noise_chunk(case.seed, case.chunk)))
		.collect();
	let fields: Vec<_> = vectors
		.fields
		.iter()
		.map(|case| hex::encode(Sha256::digest(noise_image(case.seed))))
		.collect();

	assert_eq!(
		chunks,
		vectors.chunks.iter().map(|case| case.expect_hex.clone()).collect::<Vec<_>>()
	);
	assert_eq!(
		fields,
		vectors.fields.iter().map(|case| case.expect_sha256.clone()).collect::<Vec<_>>()
	);
}

#[test]
fn the_seed_matches_the_vectors() {
	let vectors: SeedVectors = serde_json::from_str(SEEDS).unwrap();

	let actual: Vec<_> = vectors
		.cases
		.iter()
		.map(|case| {
			let seed = noise_seed(&RenderContext::PocketCard { card_id: case.id.clone() });
			(seed, hex::encode(noise_chunk(seed, 0)))
		})
		.collect();

	let expected: Vec<_> = vectors
		.cases
		.iter()
		.map(|case| (case.expect_seed, case.expect_chunk0_hex.clone()))
		.collect();
	assert_eq!(actual, expected);
}

/// The seed comes from the id that names the body, so a chat message keeps
/// its look whichever room shows it.
#[test]
fn each_context_seeds_from_the_id_that_names_its_body() {
	let seeds = [
		RenderContext::ChatMessage {
			room_id: "foobar".to_string(),
			message_id: "a".to_string(),
			message_type: "foobar".to_string(),
		},
		RenderContext::InputWidget { candidate_id: "foobar".to_string() },
		RenderContext::PocketCard { card_id: String::new() },
	]
	.map(|context| noise_seed(&context));

	assert_eq!(seeds, [0xe40c_292c, 0xbf9c_f968, 0x811c_9dc5]);
}
