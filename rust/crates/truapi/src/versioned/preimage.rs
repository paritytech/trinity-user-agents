//! Versioned wrappers for [`Preimage`](crate::api::Preimage) methods.
//!
//! v0.2 of `retain` keeps a CID or a DotNS name as well as a preimage key, keeps
//! several items under one confirmation, can renew by the day, and answers a dry
//! run with the price. A v0.1 request is one item with a key and no renewal, and a
//! v0.1 caller gets the outcome of its one item.
//!
//! v0.2 of `cacheAccount` gives the units of the prices. A v0.1 answer upgrades
//! to one price for each read and MiB-hours of retention.

use crate::{
	v01, v02,
	versioned::{FromLatest, IntoLatest},
};

truapi_macros::versioned_type! {
	pub enum RemotePreimageLookupSubscribeRequest { V1 => v01::RemotePreimageLookupSubscribeRequest }
	pub enum RemotePreimageLookupSubscribeItem { V1 => v01::RemotePreimageLookupSubscribeItem }
	pub enum RemotePreimageLookupSubscribeError { V1 => v01::GenericError }
	pub enum RemotePreimageSubmitRequest { V1 => Vec<u8> }
	pub enum RemotePreimageSubmitResponse { V1 => Vec<u8> }
	pub enum RemotePreimageSubmitError { V1 => v01::PreimageSubmitError }
	pub enum RemotePreimageReadRequest { V1 => v01::RemotePreimageReadRequest }
	pub enum RemotePreimageReadResponse { V1 => v01::RemotePreimageReadResponse }
	pub enum RemotePreimageReadError { V1 => v01::PreimageReadError }
	pub enum RemotePreimageReadTorrentRequest { V1 => v01::RemotePreimageReadTorrentRequest }
	pub enum RemotePreimageReadTorrentResponse { V1 => v01::RemotePreimageReadTorrentResponse }
	pub enum RemotePreimageCacheAccountRequest {
		V1 => v01::RemotePreimageCacheAccountRequest,
		V2 => v01::RemotePreimageCacheAccountRequest,
	}
	pub enum RemotePreimageCacheAccountResponse {
		V1 => v01::RemotePreimageCacheAccountResponse,
		V2 => v02::RemotePreimageCacheAccountResponse,
	}
	pub enum RemotePreimageCacheAccountError {
		V1 => v01::PreimageReadError,
		V2 => v01::PreimageReadError,
	}
	pub enum RemotePreimageRetainRequest {
		V1 => v01::RemotePreimageRetainRequest,
		V2 => v02::RemotePreimageRetainRequest,
	}
	pub enum RemotePreimageRetainResponse {
		V1 => v01::RemotePreimageRetainResponse,
		V2 => v02::RemotePreimageRetainResponse,
	}
	pub enum RemotePreimageRetainError {
		V1 => v01::PreimageRetainError,
		V2 => v01::PreimageRetainError,
	}
	pub enum RemotePreimageCacheProvidersRequest { V1 => v01::RemotePreimageCacheProvidersRequest }
	pub enum RemotePreimageCacheProvidersResponse { V1 => v01::RemotePreimageCacheProvidersResponse }
	pub enum RemotePreimageRetentionsRequest { V1 => v01::RemotePreimageRetentionsRequest }
	pub enum RemotePreimageRetentionsResponse { V1 => v01::RemotePreimageRetentionsResponse }
	pub enum RemotePreimageStopRetentionRequest { V1 => v01::RemotePreimageStopRetentionRequest }
	pub enum RemotePreimageStopRetentionResponse { V1 => v01::RemotePreimageStopRetentionResponse }
}

impl IntoLatest for RemotePreimageCacheAccountRequest {
	fn into_latest(self) -> Self::Latest {
		match self {
			Self::V1(request) | Self::V2(request) => request,
		}
	}
}

impl IntoLatest for RemotePreimageCacheAccountResponse {
	fn into_latest(self) -> Self::Latest {
		match self {
			Self::V1(v01::RemotePreimageCacheAccountResponse {
				payer,
				provider,
				allowance,
				allowance_retention,
				paid,
				allowance_per_day,
				retention_allowance_per_day,
				read_price,
				retention_price,
			}) => v02::RemotePreimageCacheAccountResponse {
				payer,
				provider,
				allowance,
				allowance_retention,
				paid,
				allowance_per_day,
				retention_allowance_per_day,
				read_price,
				read_price_per_mib: 0,
				retention_price,
				retention_unit_secs: 3600,
			},
			Self::V2(latest) => latest,
		}
	}
}

impl FromLatest for RemotePreimageCacheAccountResponse {
	fn from_latest(latest: Self::Latest, target: u8) -> Self {
		if target >= 2 {
			return Self::V2(latest);
		}
		Self::V1(v01::RemotePreimageCacheAccountResponse {
			payer: latest.payer,
			provider: latest.provider,
			allowance: latest.allowance,
			allowance_retention: latest.allowance_retention,
			paid: latest.paid,
			allowance_per_day: latest.allowance_per_day,
			retention_allowance_per_day: latest.retention_allowance_per_day,
			read_price: latest.read_price,
			retention_price: latest.retention_price,
		})
	}
}

impl IntoLatest for RemotePreimageCacheAccountError {
	fn into_latest(self) -> Self::Latest {
		match self {
			Self::V1(error) | Self::V2(error) => error,
		}
	}
}

impl FromLatest for RemotePreimageCacheAccountError {
	fn from_latest(latest: Self::Latest, target: u8) -> Self {
		if target >= 2 { Self::V2(latest) } else { Self::V1(latest) }
	}
}

impl IntoLatest for RemotePreimageRetainRequest {
	fn into_latest(self) -> Self::Latest {
		match self {
			Self::V1(v01::RemotePreimageRetainRequest { key, period_secs, size, provider }) => {
				v02::RemotePreimageRetainRequest {
					items: vec![v02::RetainItem {
						target: v01::PreimageTarget::Key(key),
						period_secs,
						size,
						provider,
						renew: false,
					}],
					dry_run: false,
				}
			},
			Self::V2(latest) => latest,
		}
	}
}

impl IntoLatest for RemotePreimageRetainResponse {
	fn into_latest(self) -> Self::Latest {
		match self {
			Self::V1(v01::RemotePreimageRetainResponse {
				until,
				provider,
				name,
				region,
				size,
				chunks,
				charged,
			}) => v02::RemotePreimageRetainResponse {
				items: vec![v02::RetainOutcome {
					cid: String::new(),
					provider,
					name,
					region,
					operator: None,
					kind: if chunks > 0 { v02::RetainKind::File } else { v02::RetainKind::Raw },
					size,
					pieces: chunks.saturating_add(1),
					until,
					price: charged,
					charged,
					free: 0,
					paid: 0,
					short: 0,
					renews: false,
					state: v02::RetainState::Kept,
				}],
			},
			Self::V2(latest) => latest,
		}
	}
}

impl FromLatest for RemotePreimageRetainResponse {
	fn from_latest(latest: Self::Latest, target: u8) -> Self {
		if target >= 2 {
			return Self::V2(latest);
		}
		// A v0.1 request has one item, so its answer is the outcome of that item.
		let outcome = latest.items.into_iter().next();
		Self::V1(match outcome {
			Some(outcome) => v01::RemotePreimageRetainResponse {
				until: outcome.until,
				provider: outcome.provider,
				name: outcome.name,
				region: outcome.region,
				size: outcome.size,
				chunks: outcome.pieces.saturating_sub(1),
				charged: outcome.charged,
			},
			None => v01::RemotePreimageRetainResponse {
				until: 0,
				provider: [0; 32],
				name: None,
				region: None,
				size: 0,
				chunks: 0,
				charged: 0,
			},
		})
	}
}

impl IntoLatest for RemotePreimageRetainError {
	fn into_latest(self) -> Self::Latest {
		match self {
			Self::V1(error) | Self::V2(error) => error,
		}
	}
}

impl FromLatest for RemotePreimageRetainError {
	fn from_latest(latest: Self::Latest, target: u8) -> Self {
		if target >= 2 { Self::V2(latest) } else { Self::V1(latest) }
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn a_v01_account_has_one_read_price_and_hourly_retention() {
		let v1 = v01::RemotePreimageCacheAccountResponse {
			payer: [1; 32],
			provider: [2; 32],
			allowance: 400,
			allowance_retention: 40,
			paid: 7,
			allowance_per_day: 500,
			retention_allowance_per_day: 48,
			read_price: 10,
			retention_price: 1,
		};
		let latest = RemotePreimageCacheAccountResponse::V1(v1.clone()).into_latest();
		assert_eq!(
			latest,
			v02::RemotePreimageCacheAccountResponse {
				payer: [1; 32],
				provider: [2; 32],
				allowance: 400,
				allowance_retention: 40,
				paid: 7,
				allowance_per_day: 500,
				retention_allowance_per_day: 48,
				read_price: 10,
				read_price_per_mib: 0,
				retention_price: 1,
				retention_unit_secs: 3600,
			}
		);
		assert_eq!(
			RemotePreimageCacheAccountResponse::from_latest(latest, 1),
			RemotePreimageCacheAccountResponse::V1(v1)
		);
	}

	#[test]
	fn a_v01_retention_is_one_item_with_a_key_and_no_renewal() {
		let upgraded = RemotePreimageRetainRequest::V1(v01::RemotePreimageRetainRequest {
			key: vec![7; 32],
			period_secs: 3600,
			size: Some(10),
			provider: Some([1; 32]),
		})
		.into_latest();
		assert_eq!(
			upgraded,
			v02::RemotePreimageRetainRequest {
				items: vec![v02::RetainItem {
					target: v01::PreimageTarget::Key(vec![7; 32]),
					period_secs: 3600,
					size: Some(10),
					provider: Some([1; 32]),
					renew: false,
				}],
				dry_run: false,
			}
		);
	}

	#[test]
	fn a_v01_caller_gets_the_outcome_of_its_one_item() {
		let outcome = v02::RetainOutcome {
			cid: "bafk".to_string(),
			provider: [2; 32],
			name: Some("A".to_string()),
			region: Some("Paris".to_string()),
			operator: None,
			kind: v02::RetainKind::File,
			size: 300,
			pieces: 4,
			until: 99,
			price: 5,
			charged: 5,
			free: 5,
			paid: 0,
			short: 0,
			renews: false,
			state: v02::RetainState::Kept,
		};
		let downgraded = RemotePreimageRetainResponse::from_latest(
			v02::RemotePreimageRetainResponse { items: vec![outcome] },
			1,
		);
		assert_eq!(
			downgraded,
			RemotePreimageRetainResponse::V1(v01::RemotePreimageRetainResponse {
				until: 99,
				provider: [2; 32],
				name: Some("A".to_string()),
				region: Some("Paris".to_string()),
				size: 300,
				chunks: 3,
				charged: 5,
			})
		);
	}
}
