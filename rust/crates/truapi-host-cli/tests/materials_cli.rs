//! Process-boundary coverage for `truapi-host materials resolve`: a card face's materials,
//! resolved by the same core a phone uses, on a stack whose chains are unreachable.

#[path = "common/bundled.rs"]
mod bundled;
#[path = "common/offline.rs"]
mod offline;

use std::{fs, path::Path, process::Output};

const STILL: &[u8] =
	b"// @material 1\nvec4 material(vec2 uv) {\n  return texture(r_input, uv);\n}\n";

const NO_ENTRY_POINT: &[u8] = b"// @material 1\nfloat shade(vec2 uv) {\n  return 1.0;\n}\n";

fn resolve(base: &Path, args: &[&Path]) -> Output {
	let output = offline::offline_host_command()
		.args(["materials", "resolve"])
		.args(args)
		.arg("--base-path")
		.arg(base)
		.env("TRUAPI_HOST_NO_UPDATE", "1")
		.output()
		.expect("run truapi-host");
	assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
	output
}

fn last_materials(output: &Output) -> Vec<String> {
	let stdout = String::from_utf8_lossy(&output.stdout);
	let lines: Vec<String> = stdout.lines().map(str::to_string).collect();
	let start = lines
		.iter()
		.rposition(|line| line == "materials:")
		.expect("materials are printed");
	lines[start + 1..].to_vec()
}

/// A face with one node that draws and one over the 16-layer limit, written as the generated
/// client writes a renderer tree.
fn write_face(dir: &Path) -> std::path::PathBuf {
	let layer = serde_json::json!({
		"material": format!("0x{}", hex::encode(truapi::preimage_key(STILL))),
		"args": [],
	});
	let node = |layers: Vec<serde_json::Value>| serde_json::json!({ "tag": "Materials", "value": { "props": { "layers": layers }, "children": [] } });
	let face = serde_json::json!({
		"tag": "Box",
		"value": {
			"modifiers": [],
			"props": { "contentAlignment": null },
			"children": [node(vec![layer.clone()]), node(vec![layer; 17])],
		},
	});
	let path = dir.join("face.json");
	fs::write(&path, face.to_string()).unwrap();
	path
}

/// Without a phone, this is how a material is checked end to end: kept from a file as a fetch
/// would keep it, resolved with the limits, and built into shaders a GPU compiles. What is kept
/// stays on the device, so a later run draws without the files or Bulletin.
#[test]
fn a_face_plans_from_material_files_and_later_from_what_was_kept() {
	let dir = tempfile::tempdir().unwrap();
	let materials = dir.path().join("materials");
	fs::create_dir(&materials).unwrap();
	fs::write(materials.join("still.glsl"), STILL).unwrap();
	fs::write(materials.join("broken.glsl"), NO_ENTRY_POINT).unwrap();
	let face = write_face(dir.path());
	let out = dir.path().join("out");
	let base = dir.path().join("base");

	let first =
		resolve(&base, &[&face, Path::new("--materials"), &materials, Path::new("--out"), &out]);

	let stdout = String::from_utf8_lossy(&first.stdout);
	assert!(stdout.contains("refused broken.glsl: InvalidMaterial"), "{stdout}");
	assert!(
		stdout.contains(&format!("kept still.glsl 0x{}", hex::encode(truapi::preimage_key(STILL)))),
		"{stdout}"
	);
	assert_eq!(last_materials(&first), ["node 0: ready, 1 layer", "node 1: plain, LimitExceeded"]);
	let fragment = fs::read_to_string(out.join("node0-layer0.frag")).unwrap();
	assert!(fragment.starts_with("#version 300 es"), "{fragment}");
	assert!(out.join("node0-layer0.vert").is_file());
	assert!(out.join("node0-layer0.txt").is_file());

	let later = resolve(&base, &[&face]);
	assert_eq!(last_materials(&later), ["node 0: ready, 1 layer", "node 1: plain, LimitExceeded"]);
}
