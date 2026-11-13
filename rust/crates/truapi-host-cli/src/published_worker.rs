//! A product's Worker as it is published on dotNS, fetched and unpacked so
//! the host can run it.
//!
//! The worker lives under the product's `worker.<name>.<tld>` subname: its
//! `executable` text record is the Worker manifest, whose `entrypoint` names
//! the script, and its contenthash is the archive the script is in. The
//! archive is fetched from the stack's IPFS gateway as a CAR, checked block
//! by block against the contenthash, and unpacked under the state directory
//! once per contenthash, so a worker is fetched again only once its publisher
//! changes it.

use std::{
	path::{Path, PathBuf},
	sync::Arc,
	time::Duration,
};

use anyhow::{Context, Result, bail, ensure};
use truapi::{
	host_logic::dotns_gateway::{ContentResolver, DotnsRegistry, namehash_under, read_view},
	platform::ProductId,
};

use crate::{chain::CliChainProvider, dotns_read::AssetHubReader, network::Stack};

pub mod archive;

use archive::Cid;

alloy_sol_types::sol! {
	/// The ENSIP-7 view of a dotNS content resolver.
	interface ContenthashResolver {
		/// The node's contenthash: a multicodec namespace, then a CID.
		function contenthash(bytes32 node) external view returns (bytes);
	}
}

/// Subname label a product's Worker is published under.
const WORKER_SUBNAME: &str = "worker";
/// Text record a Worker subname publishes its manifest at.
const EXECUTABLE_RECORD: &str = "executable";
/// The `ipfs` multicodec namespace a contenthash starts with.
const IPFS_NAMESPACE: u64 = 0xe3;
/// Bound on one gateway download.
const FETCH_TIMEOUT: Duration = Duration::from_secs(120);

/// Where a published worker is: the archive its contenthash names, and the
/// script inside it its manifest names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishedWorker {
	/// The archive's root.
	pub archive: Cid,
	/// The script's path inside the archive.
	pub entrypoint: String,
}

/// A published worker unpacked under the state directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnpackedWorker {
	/// The directory the archive is unpacked in.
	pub archive: PathBuf,
	/// The entry script inside it.
	pub script: PathBuf,
}

/// Fetches the published workers of one stack.
pub struct PublishedWorkers {
	chains: Arc<CliChainProvider>,
	asset_hub: [u8; 32],
	pop_controller: [u8; 20],
	registry: [u8; 20],
	tld: String,
	gateway: String,
	http: reqwest::Client,
}

impl PublishedWorkers {
	/// The published workers of `network`, reached over `chains`. `None` when
	/// the stack names no dotNS registry or IPFS gateway to read them from.
	pub fn new(network: &Stack, chains: Arc<CliChainProvider>) -> Option<Self> {
		Some(Self {
			chains,
			asset_hub: network.genesis().asset_hub,
			pop_controller: network.pop_controller(),
			registry: network.dotns_registry()?,
			tld: network.tld().to_string(),
			gateway: network.ipfs_gateway()?.to_string(),
			http: reqwest::Client::new(),
		})
	}

	/// `product_id`'s published worker, fetched into `cache` unless it
	/// already holds it.
	pub async fn unpack(&self, product_id: &ProductId, cache: &Path) -> Result<UnpackedWorker> {
		let worker = self.resolve(product_id).await?;
		let directory = cache.join(worker.archive.to_base32());
		if !directory.is_dir() {
			let car = self.fetch(&worker.archive).await?;
			store(&directory, &car)?;
		}
		let script = entry_script(&directory, &worker.entrypoint)?;
		Ok(UnpackedWorker { archive: directory, script })
	}

	/// Read `worker.<product>.<tld>`'s contenthash and manifest on dotNS.
	pub async fn resolve(&self, product_id: &ProductId) -> Result<PublishedWorker> {
		let mut reader =
			AssetHubReader::over(&self.chains, self.asset_hub, self.pop_controller).await?;
		let node = worker_node(&self.tld, product_id.bare_label());
		let resolver = read_view(
			&mut reader,
			&self.registry,
			DotnsRegistry::resolverCall { node: node.into() },
		)
		.await
		.map_err(|error| {
			anyhow::anyhow!("reading the resolver of {product_id}'s worker: {error}")
		})?;
		ensure!(!resolver.is_zero(), "{product_id} publishes no worker on dotNS");
		let resolver = resolver.into_array();
		let contenthash = read_view(
			&mut reader,
			&resolver,
			ContenthashResolver::contenthashCall { node: node.into() },
		)
		.await
		.map_err(|error| {
			anyhow::anyhow!("reading the contenthash of {product_id}'s worker: {error}")
		})?;
		let manifest = read_view(
			&mut reader,
			&resolver,
			ContentResolver::textCall { node: node.into(), key: EXECUTABLE_RECORD.to_string() },
		)
		.await
		.map_err(|error| {
			anyhow::anyhow!("reading the manifest of {product_id}'s worker: {error}")
		})?;
		Ok(PublishedWorker {
			archive: contenthash_cid(&contenthash)
				.with_context(|| format!("the contenthash of {product_id}'s worker"))?,
			entrypoint: manifest_entrypoint(&manifest)
				.with_context(|| format!("the manifest of {product_id}'s worker"))?,
		})
	}

	/// The archive `cid` names, as a CAR the gateway exports.
	async fn fetch(&self, cid: &Cid) -> Result<Vec<u8>> {
		let url = format!("{}/{}?format=car", self.gateway.trim_end_matches('/'), cid.to_base32());
		let response = self
			.http
			.get(&url)
			.header(reqwest::header::ACCEPT, "application/vnd.ipld.car")
			.timeout(FETCH_TIMEOUT)
			.send()
			.await
			.and_then(reqwest::Response::error_for_status)
			.with_context(|| format!("fetching {url}"))?;
		let car = response.bytes().await.with_context(|| format!("reading {url}"))?;
		tracing::info!(%url, size = car.len(), "fetched a published worker");
		Ok(car.to_vec())
	}
}

/// The node of `worker.<label>.<tld>`.
pub fn worker_node(tld: &str, label: &str) -> [u8; 32] {
	let tld = namehash_under(&[0; 32], tld.trim_start_matches('.'));
	namehash_under(&namehash_under(&tld, label), WORKER_SUBNAME)
}

/// The CID an ENSIP-7 contenthash names in the `ipfs` namespace.
pub fn contenthash_cid(contenthash: &[u8]) -> Result<Cid> {
	ensure!(!contenthash.is_empty(), "no contenthash is set");
	let (namespace, cid) = archive::varint(contenthash)?;
	ensure!(namespace == IPFS_NAMESPACE, "contenthash namespace {namespace:#x} is not ipfs");
	Cid::parse(cid)
}

/// The `entrypoint` a Worker manifest names: a relative path that stays
/// inside the archive.
pub fn manifest_entrypoint(manifest: &str) -> Result<String> {
	ensure!(!manifest.is_empty(), "no manifest is published");
	let manifest: serde_json::Value = serde_json::from_str(manifest).context("not JSON")?;
	let entrypoint = manifest["entrypoint"].as_str().context("no entrypoint")?;
	let entrypoint = entrypoint.trim_start_matches("./");
	ensure!(
		!entrypoint.is_empty() &&
			!entrypoint.starts_with('/') &&
			entrypoint
				.split(['/', '\\'])
				.all(|part| !part.is_empty() && part != ".." && part != "."),
		"entrypoint {entrypoint:?} leaves the archive"
	);
	Ok(entrypoint.to_string())
}

/// Unpack `car` into `directory` whole: into a staging directory first, moved
/// into place once complete, so a directory that exists holds every file.
fn store(directory: &Path, car: &[u8]) -> Result<()> {
	let files = archive::unpack(car)?;
	let parent = directory.parent().context("the cache directory has no parent")?;
	std::fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
	let staging =
		tempfile::tempdir_in(parent).with_context(|| format!("staging in {}", parent.display()))?;
	for (path, content) in files {
		let file = staging.path().join(&path);
		if let Some(parent) = file.parent() {
			std::fs::create_dir_all(parent)?;
		}
		std::fs::write(&file, content).with_context(|| format!("writing {}", file.display()))?;
	}
	let staged = staging.keep();
	if let Err(error) = std::fs::rename(&staged, directory) {
		let _ = std::fs::remove_dir_all(&staged);
		// Another launch may have stored the same archive first.
		if !directory.is_dir() {
			bail!("moving the worker into {}: {error}", directory.display());
		}
	}
	Ok(())
}

/// The script `entrypoint` names inside the unpacked `directory`.
fn entry_script(directory: &Path, entrypoint: &str) -> Result<PathBuf> {
	let script = directory.join(entrypoint);
	ensure!(script.is_file(), "the worker archive has no {entrypoint}");
	Ok(script)
}

#[cfg(test)]
mod tests;
