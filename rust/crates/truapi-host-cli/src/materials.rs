//! `truapi-host materials`: the plan a phone draws a card face's materials from, built by the
//! same core, so a material can be checked end to end without a phone.

use std::{
	fs,
	path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use clap::Subcommand;
use futures::StreamExt;
use truapi::{
	MaterialPlan, MaterialShader, NodePlan, ShaderLanguage, SigningHostConfig, SigningHostRuntime,
	Spawner, latest::RenderContext,
};

use crate::{
	network::Stack,
	platform::{ApprovalPolicy, CliPlatform},
};

#[derive(Subcommand)]
pub enum MaterialsCommand {
	/// Print the plan for each `Materials` node of a card face, until none is still loading.
	Plan {
		/// The face as JSON, as the generated client writes a renderer tree.
		face: PathBuf,
		/// A directory of material files to keep first, so the plan needs no Bulletin. What is
		/// kept stays for later runs, as it does on a phone.
		#[arg(long)]
		materials: Option<PathBuf>,
		/// Build Metal shaders, as iOS draws, instead of GLSL ES, as Android does.
		#[arg(long)]
		metal: bool,
		/// Write each ready layer's shaders, and what feeds them, here.
		#[arg(long)]
		out: Option<PathBuf>,
		/// Base directory; the kept materials live in its v2 subdirectory.
		#[arg(long = "base-path", env = "TRUAPI_HOST_BASE_PATH")]
		base_path: Option<PathBuf>,
	},
}

/// Run a `materials` command on `network`.
pub async fn run(command: MaterialsCommand, network: &Stack) -> Result<()> {
	let MaterialsCommand::Plan { face, materials, metal, out, base_path } = command;
	let json = fs::read_to_string(&face).with_context(|| format!("reading {}", face.display()))?;
	let face = truapi::native::parse_renderer_node_json(json).context("reading the face")?;
	let state = crate::state_base_path(base_path).join(network.namespace()).join("materials");
	let runtime = runtime(network, &state).await?;
	if let Some(directory) = materials {
		keep_all(&runtime, &directory).await?;
	}
	let language = if metal { ShaderLanguage::Metal } else { ShaderLanguage::GlslEs };
	let card = RenderContext::PocketCard { card_id: "cli".to_string() };
	let mut plans = runtime.material_plan(&face, &card, language, false, Box::new(|_| {}));
	while let Some(plan) = plans.next().await {
		print_plan(&plan);
		if !plan.nodes.contains(&NodePlan::Loading) {
			if let Some(out) = out {
				write_shaders(&plan, &out, metal)?;
			}
			break;
		}
	}
	Ok(())
}

/// A runtime that draws materials, kept in their own database under `state`, apart from any
/// signing host's.
async fn runtime(network: &Stack, state: &Path) -> Result<SigningHostRuntime> {
	fs::create_dir_all(state).with_context(|| format!("creating {}", state.display()))?;
	let db = truapi::store::Db::open(truapi::store::core_db_config(state))
		.await
		.with_context(|| crate::core_db_cleanup_request(state))?;
	let genesis = network.genesis();
	let mut config = SigningHostConfig::new(
		crate::host_info("Materials Planner"),
		crate::platform_info(),
		genesis.people,
		genesis.bulletin,
		genesis.asset_hub,
		network.tld().to_string(),
	)
	.context("invalid signing host config")?;
	config.materials_enabled = true;
	let platform = CliPlatform::new(network, None, ApprovalPolicy::Prompt, None);
	let runtime =
		SigningHostRuntime::new(platform, config, Spawner::new(tokio::runtime::Handle::current()));
	runtime.set_core_db(db);
	runtime.start_core_services();
	Ok(runtime)
}

async fn keep_all(runtime: &SigningHostRuntime, directory: &Path) -> Result<()> {
	let mut files = fs::read_dir(directory)
		.with_context(|| format!("reading {}", directory.display()))?
		.map(|entry| entry.map(|entry| entry.path()))
		.collect::<Result<Vec<_>, _>>()?;
	files.retain(|path| path.is_file());
	files.sort();
	for path in files {
		let name = path.file_name().unwrap_or_default().to_string_lossy();
		let text = fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
		match runtime.keep_material(text).await {
			Ok(id) => println!("kept {name} 0x{}", hex::encode(id)),
			Err(reason) => println!("refused {name}: {reason:?}"),
		}
	}
	Ok(())
}

fn print_plan(plan: &MaterialPlan) {
	println!("plan:");
	for (index, node) in plan.nodes.iter().enumerate() {
		match node {
			NodePlan::Loading => println!("node {index}: loading"),
			NodePlan::Ready { layers } if layers.len() == 1 => {
				println!("node {index}: ready, 1 layer")
			},
			NodePlan::Ready { layers } => println!("node {index}: ready, {} layers", layers.len()),
			NodePlan::Plain { reason } => println!("node {index}: plain, {reason:?}"),
		}
	}
}

fn write_shaders(plan: &MaterialPlan, out: &Path, metal: bool) -> Result<()> {
	fs::create_dir_all(out).with_context(|| format!("creating {}", out.display()))?;
	let (vertex, fragment) =
		if metal { ("vertex.metal", "fragment.metal") } else { ("vert", "frag") };
	for (index, node) in plan.nodes.iter().enumerate() {
		let NodePlan::Ready { layers } = node else { continue };
		for (number, layer) in layers.iter().enumerate() {
			let name = format!("node{index}-layer{number}");
			let shader = &layer.shader;
			fs::write(out.join(format!("{name}.{vertex}")), &shader.vertex)?;
			fs::write(out.join(format!("{name}.{fragment}")), &shader.fragment)?;
			fs::write(out.join(format!("{name}.txt")), feeds(shader))?;
		}
	}
	Ok(())
}

/// What feeds `shader`, for whoever wires it into a renderer.
fn feeds(shader: &MaterialShader) -> String {
	format!(
		"vertex block: {} bytes\nfragment block: {} bytes\nreads time: {}\nreads motion: {}\nuniforms: {:#?}\ntextures: {:#?}\n",
		shader.vertex_block_size,
		shader.fragment_block_size,
		shader.reads_time,
		shader.reads_motion,
		shader.uniforms,
		shader.textures
	)
}
