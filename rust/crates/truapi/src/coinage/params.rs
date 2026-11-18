//! A3 · Chain parameters: the per-network configuration, the parameters the
//! layer reads from the runtime, and `validate`, which checks one against the
//! other.
//!
//! No component takes a chain limit from anywhere but [`ChainParams`], so a
//! runtime change can break only this module.

use core::time::Duration;

use scale_info::{PortableRegistry, TypeDef};
use subxt::{Metadata, ext::scale_decode::DecodeAsType};

use super::{Exponent, chain};
use crate::durable::MAX_REBUILD_COOLDOWN;

/// The configuration of one network, fixed when the layer is constructed. It
/// names the coinage instance the layer serves and gives every limit the
/// runtime does not publish.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChainConfig {
	/// The coinage instance the layer serves. The runtime cannot say which of
	/// its instances that is.
	pub instance: u32,
	/// The extension pipeline that carries the coinage origins.
	pub extension_version: u8,
	/// How long a full recycler ring stays usable before it expires.
	pub ring_lifetime: Duration,
	/// How many members one page of a ring holds (B1 invariant 4).
	pub ring_page_size: u32,
	/// How long a period's free tokens stay accepted after the period ends.
	pub token_grace: Duration,
	/// The external asset's minimum balance (F3 invariant 5).
	pub external_minimum_balance: u128,
	/// The time between two blocks, to compare windows with times.
	pub block_time: Duration,
	/// How long before its ring expires an entry is left to the rescue sweep
	/// (F2 invariant 1).
	pub rescue_margin: Duration,
	/// The percentage of the allowance kept as the token reserve (D1
	/// invariant 2).
	pub reserve_share: u8,
	/// The age cutoff when the runtime reports no maximum coin age.
	pub fallback_age_cutoff: u16,
	/// The longest validity window, in blocks, the layer gives a transaction.
	pub longest_window: u32,
	/// The protocol's maximum claim validity window, in blocks.
	pub max_claim_window: u32,
}

impl ChainConfig {
	/// The limits the coinage pallet does not publish, as the shipped
	/// pallet sets them, for `instance` on the People chain.
	pub fn shipped(instance: u32) -> Self {
		Self {
			instance,
			extension_version: 0,
			ring_lifetime: Duration::from_secs(90 * 24 * 3600),
			ring_page_size: 255,
			token_grace: Duration::from_secs(3600),
			external_minimum_balance: 1,
			block_time: Duration::from_secs(6),
			rescue_margin: Duration::from_secs(7 * 24 * 3600),
			reserve_share: 20,
			fallback_age_cutoff: 14,
			longest_window: 64,
			max_claim_window: 256,
		}
	}
}

/// The part of one coinage instance's record the layer reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstanceRecord {
	/// The amount of the external asset in one cent.
	pub asset_unit: u128,
}

/// The validated parameters. Only [`validate`] produces a value, so holding
/// one means every check passed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChainParams {
	config: ChainConfig,
	largest_exponent: Exponent,
	max_split_outputs: u32,
	max_unload_entries: u32,
	max_load_entries: u32,
	ring_exponent: u8,
	age_cutoff: u16,
	token_period: Duration,
	lock_period: Duration,
	instance: InstanceRecord,
}

impl ChainParams {
	/// The configuration the parameters were validated against.
	pub fn config(&self) -> &ChainConfig {
		&self.config
	}

	/// The largest legal exponent. The smallest one the layer uses is 0.
	pub fn largest_exponent(&self) -> Exponent {
		self.largest_exponent
	}

	/// Every legal exponent, smallest first.
	pub fn exponents(&self) -> impl Iterator<Item = Exponent> {
		(0..=self.largest_exponent.get()).filter_map(Exponent::new)
	}

	/// The most coins one split, or one unload's reload, may create.
	pub fn max_split_outputs(&self) -> u32 {
		self.max_split_outputs
	}

	/// The most entries one unload may present.
	pub fn max_unload_entries(&self) -> u32 {
		self.max_unload_entries
	}

	/// The most entries one load from the external asset may create.
	pub fn max_load_entries(&self) -> u32 {
		self.max_load_entries
	}

	/// How many members a recycler ring of any exponent holds.
	pub fn ring_size(&self) -> u32 {
		(1u32 << self.ring_exponent) - 257
	}

	/// The exponent `x` of a recycler ring's size class: `2^x − 257`
	/// members, proved in a ring-VRF domain of size `2^(x + 2)`.
	pub fn ring_exponent(&self) -> u8 {
		self.ring_exponent
	}

	/// The coin age from which the layer no longer spends a coin and only
	/// recycles it.
	pub fn age_cutoff(&self) -> u16 {
		self.age_cutoff
	}

	/// The length of one free-token period.
	pub fn token_period(&self) -> Duration {
		self.token_period
	}

	/// The lock after a coin's first failed dispatch; it doubles with each
	/// failure that follows.
	pub fn lock_period(&self) -> Duration {
		self.lock_period
	}

	/// The record of the instance the layer serves.
	pub fn instance(&self) -> &InstanceRecord {
		&self.instance
	}
}

/// The runtime cannot serve the layer, and the check that failed.
#[derive(Clone, Debug, PartialEq, Eq, derive_more::Display, derive_more::Error)]
#[display("unsupported runtime: {check}")]
pub struct UnsupportedRuntime {
	/// The check that failed.
	pub check: String,
}

impl UnsupportedRuntime {
	fn new(check: impl Into<String>) -> Self {
		Self { check: check.into() }
	}
}

/// Checks the runtime against `config` and returns the parameters the layer
/// runs with.
///
/// `runtime` is the runtime description, `instance` the record of the
/// configured instance (none when the chain has no such instance), and
/// `maximum_age` the maximum coin age the runtime reports (none when it
/// reports none, or its read failed). Calls and origins are looked up by name
/// (invariant 1); the age cutoff is two below `maximum_age`, or the configured
/// fallback (invariant 2); and the relations of invariant 3 are checked.
pub fn validate(
	runtime: &Metadata,
	instance: Option<&InstanceRecord>,
	maximum_age: Option<u16>,
	config: ChainConfig,
) -> Result<ChainParams, UnsupportedRuntime> {
	let pallet = runtime
		.pallet_by_name(chain::PALLET)
		.ok_or_else(|| UnsupportedRuntime::new(format!("no pallet {}", chain::PALLET)))?;
	for call in chain::CALLS {
		if pallet.call_variant_by_name(call).is_none() {
			return Err(UnsupportedRuntime::new(format!("no call {}::{call}", chain::PALLET)));
		}
	}
	if pallet.view_function_by_name(chain::FREE_TOKEN_INFO).is_none() {
		return Err(UnsupportedRuntime::new(format!("no view function {}::{}", chain::PALLET, chain::FREE_TOKEN_INFO)));
	}
	check_origins(runtime, config.extension_version)?;

	let constant = |name: &str| {
		pallet
			.constant_by_name(name)
			.map(|constant| (constant.value(), constant.ty()))
			.ok_or_else(|| UnsupportedRuntime::new(format!("no constant {name}")))
	};
	let number = |name: &str| -> Result<i128, UnsupportedRuntime> {
		let (mut bytes, ty) = constant(name)?;
		i128::decode_as_type(&mut bytes, ty, runtime.types())
			.map_err(|error| UnsupportedRuntime::new(format!("constant {name}: {error}")))
	};
	let count = |name: &str| -> Result<u32, UnsupportedRuntime> {
		u32::try_from(number(name)?)
			.map_err(|_| UnsupportedRuntime::new(format!("constant {name} is out of range")))
	};
	let seconds = |name: &str| -> Result<Duration, UnsupportedRuntime> {
		u64::try_from(number(name)?)
			.map(Duration::from_secs)
			.map_err(|_| UnsupportedRuntime::new(format!("constant {name} is out of range")))
	};

	let smallest = number(chain::MINIMUM_EXPONENT)?;
	let largest = number(chain::MAXIMUM_EXPONENT)?;
	if smallest > 0 {
		return Err(UnsupportedRuntime::new("the smallest legal exponent is above 0"));
	}
	let largest_exponent = u8::try_from(largest)
		.ok()
		.and_then(Exponent::new)
		.ok_or_else(|| UnsupportedRuntime::new("the largest legal exponent is out of range"))?;
	let legal_exponents = u32::from(largest_exponent.get()) + 1;

	let max_split_outputs = count(chain::MAX_SPLIT_OUTPUTS)?;
	if max_split_outputs <= u32::from(largest_exponent.get()) {
		return Err(UnsupportedRuntime::new("the per-split output cap does not exceed the exponent range"));
	}
	if max_split_outputs < legal_exponents {
		return Err(UnsupportedRuntime::new("the per-load cap is below the number of legal exponents"));
	}
	let ring_exponent = ring_exponent(runtime, &constant)?;
	let ring_size = (1u32 << ring_exponent) - 257;
	let max_unload_entries = count(chain::MAX_CONSOLIDATION)?.min(ring_size);
	let max_load_entries = count(chain::MAX_BATCH_UNPAID_LOAD)?;
	let token_period = seconds(chain::TOKEN_PERIOD)?;
	let lock_period = seconds(chain::COIN_FAILURE_LOCK_PERIOD)?;

	let instance = instance
		.cloned()
		.ok_or_else(|| UnsupportedRuntime::new(format!("no instance {}", config.instance)))?;
	check_configuration(&config, token_period)?;
	let age_cutoff = maximum_age.map_or(config.fallback_age_cutoff, |maximum| maximum.saturating_sub(2));

	Ok(ChainParams {
		config,
		largest_exponent,
		max_split_outputs,
		max_unload_entries,
		max_load_entries,
		ring_exponent,
		age_cutoff,
		token_period,
		lock_period,
		instance,
	})
}

/// Checks that pipeline `extension_version` carries the coinage origins and
/// the signature a coin origin needs, and that the origins exist by name.
fn check_origins(runtime: &Metadata, extension_version: u8) -> Result<(), UnsupportedRuntime> {
	let pipeline = runtime
		.extrinsic()
		.transaction_extensions_by_version(extension_version)
		.ok_or_else(|| UnsupportedRuntime::new(format!("no extension pipeline {extension_version}")))?
		.map(|extension| (extension.identifier().to_owned(), extension.extra_ty()))
		.collect::<Vec<_>>();
	let declared = |name: &str| pipeline.iter().find(|(identifier, _)| identifier == name);
	if declared(chain::VERIFY_SIGNATURE).is_none() {
		return Err(UnsupportedRuntime::new(format!(
			"pipeline {extension_version} has no {}",
			chain::VERIFY_SIGNATURE
		)));
	}
	let (_, origin_type) = declared(chain::EXTENSION).ok_or_else(|| {
		UnsupportedRuntime::new(format!("pipeline {extension_version} has no {}", chain::EXTENSION))
	})?;
	let origins = origin_names(runtime.types(), *origin_type)
		.ok_or_else(|| UnsupportedRuntime::new(format!("{} names no origins", chain::EXTENSION)))?;
	for origin in chain::ORIGINS {
		if !origins.iter().any(|name| name == origin) {
			return Err(UnsupportedRuntime::new(format!("no origin {}::{origin}", chain::EXTENSION)));
		}
	}
	Ok(())
}

/// The variant names of the origin enum inside the extension's value, which
/// is wrapped in a single-field struct and an `Option`.
fn origin_names(types: &PortableRegistry, mut ty: u32) -> Option<Vec<String>> {
	const MAX_WRAPPERS: usize = 4;
	for _ in 0..MAX_WRAPPERS {
		match &types.resolve(ty)?.type_def {
			TypeDef::Composite(composite) if composite.fields.len() == 1 => {
				ty = composite.fields[0].ty.id;
			},
			TypeDef::Variant(variant) => {
				let some = variant
					.variants
					.iter()
					.find(|candidate| candidate.name == "Some" && candidate.fields.len() == 1);
				match some {
					Some(some) if variant.variants.len() == 2 => ty = some.fields[0].ty.id,
					_ => return Some(variant.variants.iter().map(|v| v.name.clone()).collect()),
				}
			},
			_ => return None,
		}
	}
	None
}

/// The ring exponent `x` the runtime names for recycler rings, which hold
/// `2^x − 257` members.
fn ring_exponent<'a>(
	runtime: &Metadata,
	constant: &impl Fn(&str) -> Result<(&'a [u8], u32), UnsupportedRuntime>,
) -> Result<u8, UnsupportedRuntime> {
	let (bytes, ty) = constant(chain::RECYCLER_RING_EXPONENT)?;
	let unknown = || UnsupportedRuntime::new(format!("unknown {}", chain::RECYCLER_RING_EXPONENT));
	let index = *bytes.first().ok_or_else(unknown)?;
	let TypeDef::Variant(variant) = &runtime.types().resolve(ty).ok_or_else(unknown)?.type_def
	else {
		return Err(unknown());
	};
	let name = &variant.variants.iter().find(|v| v.index == index).ok_or_else(unknown)?.name;
	let exponent: u8 = name.strip_prefix("R2e").and_then(|x| x.parse().ok()).ok_or_else(unknown)?;
	(9..=16).contains(&exponent).then_some(exponent).ok_or_else(unknown)
}

/// The checks on the configuration alone and against the token period
/// (invariants 2 and 3).
fn check_configuration(config: &ChainConfig, token_period: Duration) -> Result<(), UnsupportedRuntime> {
	let window = config.longest_window;
	if !window.is_power_of_two() || !(4..=65_536).contains(&window) {
		return Err(UnsupportedRuntime::new("the longest validity window is not a power of two from 4 to 65536"));
	}
	if window > config.max_claim_window {
		return Err(UnsupportedRuntime::new("the longest validity window exceeds the maximum claim validity window"));
	}
	if config.reserve_share > 100 {
		return Err(UnsupportedRuntime::new("the token reserve share is above 100 percent"));
	}
	let longest_wait = token_period + config.block_time * window + MAX_REBUILD_COOLDOWN;
	if config.rescue_margin <= longest_wait {
		return Err(UnsupportedRuntime::new(
			"the rescue margin does not exceed a token period, the longest validity window and the longest rebuild wait",
		));
	}
	Ok(())
}

#[cfg(test)]
mod tests;
