//! Bundled network catalog.
//!
//! A network is a relay chain plus its system parachains, with the relay
//! topology and statement-store placement fixed here so hosts register a whole
//! network by name instead of assembling specs, relay wiring, and genesis
//! hashes themselves. Only networks that ship a light-sync checkpoint are
//! bundled (a checkpointless spec carries full genesis storage — megabytes
//! unfit for a light-client binary); other networks are supplied per chain via
//! [`ChainSource::light_client`](crate::ChainSource::light_client).

use std::collections::HashMap;

use crate::{ProviderError, config::ChainSource, provider::EmbeddedChainProviderBuilder};

/// One chain within a [`NetworkDef`], identified by the genesis hash its spec
/// derives to.
struct ChainDef {
	/// Chain-spec JSON.
	spec: &'static str,
	/// Whether the statement-store networking protocol runs on this chain.
	statement_protocol: bool,
}

/// A relay chain and its system parachains.
struct NetworkDef {
	name: &'static str,
	relay: ChainDef,
	assethub: ChainDef,
	bulletin: ChainDef,
	people: ChainDef,
}

/// Genesis hashes of a registered network's chains, returned by
/// [`EmbeddedChainProviderBuilder::add_network`] so the host knows what to
/// [`connect`](crate::EmbeddedChainProvider) to.
#[derive(Debug, Clone)]
pub struct NetworkChains {
	/// Relay-chain genesis hash.
	pub relay: [u8; 32],
	/// Asset Hub genesis hash.
	pub assethub: [u8; 32],
	/// Bulletin-chain genesis hash.
	pub bulletin: [u8; 32],
	/// People-chain genesis hash.
	pub people: [u8; 32],
}

/// The Paseo relay chain, shared by every network built on Paseo.
const PASEO_RELAY: ChainDef =
	ChainDef { spec: include_str!("../networks/paseo.json"), statement_protocol: false };

/// The bundled networks, in lookup order.
///
/// A relay can back more than one network, as [`PASEO_RELAY`] does. Looking a
/// chain up by genesis hash ([`catalog_network_chains`], [`catalog_service`])
/// returns the first network that lists it, so the shared relay resolves and
/// is attributed to the earliest of them. That choice does not change how the
/// relay syncs, because every network carries the same relay definition, and
/// each parachain still resolves to the one network that lists it. Append new
/// networks so that existing lookups keep their answer.
const CATALOG: &[NetworkDef] = &[
	NetworkDef {
		name: "paseo-next-v2",
		relay: PASEO_RELAY,
		assethub: ChainDef {
			spec: include_str!("../networks/paseo-next-v2-asset-hub.json"),
			statement_protocol: false,
		},
		bulletin: ChainDef {
			spec: include_str!("../networks/paseo-next-v2-bulletin.json"),
			statement_protocol: false,
		},
		people: ChainDef {
			spec: include_str!("../networks/paseo-next-v2-people.json"),
			statement_protocol: true,
		},
	},
	NetworkDef {
		name: "previewnet",
		relay: ChainDef {
			spec: include_str!("../networks/previewnet.json"),
			statement_protocol: false,
		},
		assethub: ChainDef {
			spec: include_str!("../networks/previewnet-asset-hub.json"),
			statement_protocol: false,
		},
		bulletin: ChainDef {
			spec: include_str!("../networks/previewnet-bulletin.json"),
			statement_protocol: false,
		},
		people: ChainDef {
			spec: include_str!("../networks/previewnet-people.json"),
			statement_protocol: true,
		},
	},
	NetworkDef {
		name: "paseo",
		relay: PASEO_RELAY,
		assethub: ChainDef {
			spec: include_str!("../networks/paseo-asset-hub.json"),
			statement_protocol: false,
		},
		bulletin: ChainDef {
			spec: include_str!("../networks/paseo-bulletin.json"),
			statement_protocol: false,
		},
		people: ChainDef {
			spec: include_str!("../networks/paseo-people.json"),
			statement_protocol: true,
		},
	},
];

/// Names of the bundled networks.
pub fn known_networks() -> impl Iterator<Item = &'static str> {
	CATALOG.iter().map(|network| network.name)
}

fn genesis(chain: &ChainDef) -> Result<[u8; 32], trinity_stack::Error> {
	trinity_stack::light_spec_genesis(chain.spec.as_bytes())
}

/// A network's genesis hashes paired with each chain's genesis hash and
/// [`ChainSource`].
type NetworkSources = (NetworkChains, Vec<([u8; 32], ChainSource)>);

/// A resolved catalog network: chains keyed by genesis hash, plus the relay the
/// queried chain syncs through (`None` when it is itself the relay).
pub type ResolvedNetwork = (HashMap<[u8; 32], ChainSource>, Option<[u8; 32]>);

/// Genesis hashes and standalone per-chain [`ChainSource`]s of one network; the
/// caller pairs each parachain with the network's relay genesis.
fn network_sources(network: &NetworkDef) -> Result<NetworkSources, ProviderError> {
	let genesis = |service, chain| {
		genesis(chain).map_err(|source| ProviderError::MalformedBundledSpec {
			network: network.name,
			service,
			source,
		})
	};
	let chains = NetworkChains {
		relay: genesis("relay", &network.relay)?,
		assethub: genesis("asset-hub", &network.assethub)?,
		bulletin: genesis("bulletin", &network.bulletin)?,
		people: genesis("people", &network.people)?,
	};
	let sources = vec![
		(chains.relay, light_source(&network.relay)),
		(chains.assethub, light_source(&network.assethub)),
		(chains.bulletin, light_source(&network.bulletin)),
		(chains.people, light_source(&network.people)),
	];
	Ok((chains, sources))
}

/// Register every chain of the bundled network `name`, wiring the parachains
/// to the relay and enabling the statement-store protocol where the catalog
/// specifies it. Returns the chains' genesis hashes.
pub fn add_network(
	builder: EmbeddedChainProviderBuilder,
	name: &str,
) -> Result<(EmbeddedChainProviderBuilder, NetworkChains), ProviderError> {
	let network = CATALOG.iter().find(|network| network.name == name).ok_or_else(|| {
		ProviderError::UnknownNetwork {
			name: name.to_owned(),
			known: known_networks().collect::<Vec<_>>().join(", "),
		}
	})?;

	let (chains, sources) = network_sources(network)?;
	let relay_genesis = chains.relay;
	let builder = sources.into_iter().fold(builder, |builder, (genesis_hash, source)| {
		if genesis_hash == relay_genesis {
			builder.chain(genesis_hash, source)
		} else {
			builder.parachain(genesis_hash, source, relay_genesis)
		}
	});
	Ok((builder, chains))
}

/// The network name and service (`relay`/`asset-hub`/`bulletin`/`people`) that
/// `genesis_hash` maps to in the catalog, for log attribution. `None` if no
/// bundled network defines it.
pub fn catalog_service(genesis_hash: [u8; 32]) -> Option<(&'static str, &'static str)> {
	for network in CATALOG {
		for (service, chain) in [
			("relay", &network.relay),
			("asset-hub", &network.assethub),
			("bulletin", &network.bulletin),
			("people", &network.people),
		] {
			if genesis(chain).ok() == Some(genesis_hash) {
				return Some((network.name, service));
			}
		}
	}
	None
}

/// Resolve the bundled network containing `genesis_hash` from that hash alone:
/// its chains and the relay `genesis_hash` syncs through. `None` if no bundled
/// network defines it.
pub fn catalog_network_chains(genesis_hash: [u8; 32]) -> Option<ResolvedNetwork> {
	for network in CATALOG {
		let Ok((chains, sources)) = network_sources(network) else {
			continue;
		};
		if sources.iter().any(|(hash, _)| *hash == genesis_hash) {
			// The relay syncs on its own; a parachain syncs through the relay.
			let relay = (genesis_hash != chains.relay).then_some(chains.relay);
			return Some((sources.into_iter().collect(), relay));
		}
	}
	None
}

fn light_source(chain: &ChainDef) -> ChainSource {
	ChainSource::LightClient {
		specification: chain.spec.into(),
		database_content: None,
		statement_protocol: chain.statement_protocol,
	}
}

impl EmbeddedChainProviderBuilder {
	/// Register every chain of the bundled network `name` (see
	/// [`known_networks`]). Returns the builder and the network's genesis
	/// hashes. Errors when `name` is not bundled.
	pub fn add_network(self, name: &str) -> Result<(Self, NetworkChains), ProviderError> {
		add_network(self, name)
	}
}

#[cfg(test)]
mod tests;
