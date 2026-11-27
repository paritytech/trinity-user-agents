//! The built-in networks: each stack's link and the copy of its descriptor
//! this build ships, which the core starts from until it holds a newer one.

use std::{collections::HashMap, sync::LazyLock};

use anyhow::{Result, bail};
use clap::ValueEnum;
use trinity_stack::{DEFAULT_LINK_BASE, Descriptor, Pointer};

use super::{Network, stack::routes};
use crate::stack_descriptor::{light_client_chains, pointer_of};

/// A known stack this build ships a signed copy of, so a command starts
/// without the published descriptor when it is unreachable.
/// `scripts/update-stacks.sh` refreshes `stacks/<name>.json`.
struct BuiltInStack {
	name: &'static str,
	bundle: &'static [u8],
}

impl BuiltInStack {
	fn pointer(&self) -> Pointer {
		trinity_stack::known_stack(self.name).expect("built-in stack is not a known stack")
	}

	fn open_bundled(&self) -> Descriptor {
		trinity_stack::open(self.bundle, &self.pointer())
			.expect("invalid bundled stack")
			.descriptor
	}
}

const PASEO_NEXT_V2_STACK: BuiltInStack = BuiltInStack {
	name: "paseo-next-v2",
	bundle: include_bytes!("../../../../../stacks/paseo-next-v2.json"),
};

const PREVIEWNET_STACK: BuiltInStack = BuiltInStack {
	name: "previewnet",
	bundle: include_bytes!("../../../../../stacks/previewnet.json"),
};

const PCF_DEVNET_STACK: BuiltInStack = BuiltInStack {
	name: "pcf-devnet",
	bundle: include_bytes!("../../../../../stacks/pcf-devnet.json"),
};

/// Real Polkadot's descriptor, signed by PCF. Not a network the CLI runs on:
/// it lists Polkadot's chains and the RPC nodes that tell a fork of Polkadot
/// from Polkadot itself. Only the bundled copy is read;
/// `scripts/update-stacks.sh` refreshes it with the built-ins.
const POLKADOT_REFERENCE_STACK: BuiltInStack = BuiltInStack {
	name: "polkadot",
	bundle: include_bytes!("../../../../../stacks/polkadot.json"),
};

/// The bundled copy of real Polkadot's descriptor, opened once per process.
pub fn polkadot_reference() -> &'static Descriptor {
	static REFERENCE: LazyLock<Descriptor> =
		LazyLock::new(|| POLKADOT_REFERENCE_STACK.open_bundled());
	&REFERENCE
}

/// The key of the built-in stack published at `stack`, when `stack` is that URL with no key of
/// its own. A built-in stack named by its bare URL is then verified, and is the same stack as
/// under its network name.
pub fn built_in_key(stack: &str) -> Option<String> {
	let unsigned = pointer_of(stack, None)
		.ok()
		.flatten()
		.filter(|pointer| pointer.key().is_none())?;
	Network::value_variants().iter().find_map(|network| {
		let built_in = network.built_in_stack().pointer();
		let published = Pointer::new(built_in.url().as_str(), None).ok()?;
		let key = built_in.key().filter(|_| published.same_stack(&unsigned))?;
		Some(format!("0x{}", hex::encode(key)))
	})
}

/// The name of the built-in network whose stack has `stack_id`, or `None` for
/// any other stack.
pub fn built_in_name(stack_id: &str) -> Option<&'static str> {
	Network::of_stack(stack_id).map(|network| network.built_in_stack().name)
}

/// Fails when `descriptor`, fetched from `pointer`, is a built-in network's
/// stack the CLI cannot run on over either chain backend, so a bundled copy
/// cannot start every command with an error.
pub fn ensure_built_in_runnable(pointer: &Pointer, descriptor: &Descriptor) -> Result<()> {
	let stack_id = pointer.stack_id();
	if built_in_name(&stack_id).is_none() {
		return Ok(());
	}
	let url = pointer.url();
	if let Err(error) = routes(descriptor) {
		bail!("{url} is a built-in network's stack the CLI cannot run: {error}");
	}
	if let Some(chain) = light_client_chains(descriptor).find(|chain| chain.light_spec.is_none()) {
		let role = &chain.role;
		bail!(
			"{url} is a built-in network's stack the CLI cannot run on the light client: its \
			 descriptor names no lightSpec for {role}"
		);
	}
	Ok(())
}

/// The light spec with `sha256`, as a descriptor writes it, when a built-in
/// network's bundled copy names it, so the light client starts without the
/// spec's server.
pub fn bundled_light_spec(sha256: &str) -> Option<&'static str> {
	static SPECS: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
		Network::value_variants()
			.iter()
			.flat_map(|network| {
				serde_json::from_slice::<HashMap<String, String>>(network.bundled_light_specs())
					.expect("invalid bundled light specs")
			})
			.collect()
	});
	SPECS.get(sha256).map(String::as_str)
}

impl Network {
	fn built_in_stack(self) -> BuiltInStack {
		match self {
			Self::PaseoNextV2 => PASEO_NEXT_V2_STACK,
			Self::Previewnet => PREVIEWNET_STACK,
			Self::PcfDevnet => PCF_DEVNET_STACK,
		}
	}

	/// The light specs the bundled copy names, by `sha256`, as `stack bundle
	/// --light-specs` writes them. `scripts/update-stacks.sh` refreshes them
	/// with the copy.
	fn bundled_light_specs(self) -> &'static [u8] {
		match self {
			Self::PaseoNextV2 => {
				include_bytes!("../../../../../stacks/light-specs/paseo-next-v2.json")
			},
			Self::Previewnet => include_bytes!("../../../../../stacks/light-specs/previewnet.json"),
			Self::PcfDevnet => include_bytes!("../../../../../stacks/light-specs/pcf-devnet.json"),
		}
	}

	/// The network's link, and the signed copy of its descriptor this build
	/// ships.
	pub fn link_and_bundle(self) -> (String, &'static [u8]) {
		let built_in = self.built_in_stack();
		(built_in.pointer().to_link(DEFAULT_LINK_BASE), built_in.bundle)
	}

	/// The built-in network whose stack has `stack_id`, or `None` for any
	/// other stack.
	pub fn of_stack(stack_id: &str) -> Option<Self> {
		Self::value_variants()
			.iter()
			.copied()
			.find(|network| network.built_in_stack().pointer().stack_id() == stack_id)
	}

	/// The bundled copy of this network's stack, connected to over
	/// `chain_backend`, without fetching or storing anything.
	#[cfg(test)]
	pub fn bundled_stack(self, chain_backend: crate::chain::ChainBackend) -> super::Stack {
		let chain_access = super::testing::test_access(chain_backend);
		let (link, bundled) = self.link_and_bundle();
		let allowed = truapi::platform::ReservedValuesPolicy::ALLOWED;
		let core = truapi::platform::Stack::bundled(&link, bundled, &allowed)
			.expect("invalid bundled stack");
		let descriptor = self.built_in_stack().open_bundled();
		super::Stack::new(core.without_relay(), None, descriptor, &chain_access)
			.expect("invalid bundled stack")
	}
}

#[cfg(test)]
mod tests;
