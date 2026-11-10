//! Versioned wrappers for [`Payment`](crate::api::Payment) methods.
//!
//! v0.2 lets a funding provider's worker name the funding session a top-up or
//! payment is for, and refuses one that session does not take with
//! `FundingNotServed`. A v0.1 caller names no session, so upgrading its
//! request fills `funding` with `None`, which is what v0.1 meant.

use crate::{
	v01, v02,
	versioned::{FromLatest, IntoLatest},
};

truapi_macros::versioned_type! {
	pub enum HostPaymentBalanceSubscribeRequest { V1 => v01::HostPaymentBalanceSubscribeRequest }
	pub enum HostPaymentBalanceSubscribeItem { V1 => v01::HostPaymentBalanceSubscribeItem }
	pub enum HostPaymentBalanceSubscribeError { V1 => v01::HostPaymentBalanceSubscribeError }
	pub enum HostPaymentTopUpRequest {
		V1 => v01::HostPaymentTopUpRequest,
		V2 => v02::HostPaymentTopUpRequest,
	}
	pub enum HostPaymentTopUpResponse { V1, V2 }
	pub enum HostPaymentTopUpError {
		V1 => v01::HostPaymentTopUpError,
		V2 => v02::HostPaymentTopUpError,
	}
	pub enum HostPaymentTopUpStatusSubscribeRequest { V1 => v01::HostPaymentTopUpStatusSubscribeRequest }
	pub enum HostPaymentTopUpStatusSubscribeItem { V1 => v01::HostPaymentTopUpStatusSubscribeItem }
	pub enum HostPaymentTopUpStatusSubscribeError { V1 => v01::HostPaymentTopUpStatusSubscribeError }
	pub enum HostPaymentRequest {
		V1 => v01::HostPaymentRequest,
		V2 => v02::HostPaymentRequest,
	}
	pub enum HostPaymentResponse { V1, V2 }
	pub enum HostPaymentError {
		V1 => v01::HostPaymentError,
		V2 => v02::HostPaymentError,
	}
	pub enum HostPaymentStatusSubscribeRequest { V1 => v01::HostPaymentStatusSubscribeRequest }
	pub enum HostPaymentStatusSubscribeItem { V1 => v01::HostPaymentStatusSubscribeItem }
	pub enum HostPaymentStatusSubscribeError { V1 => v01::HostPaymentStatusSubscribeError }
}

impl IntoLatest for HostPaymentTopUpRequest {
	fn into_latest(self) -> Self::Latest {
		match self {
			Self::V1(v01::HostPaymentTopUpRequest { into, amount, source, id }) => {
				v02::HostPaymentTopUpRequest { into, amount, source, id, funding: None }
			},
			Self::V2(latest) => latest,
		}
	}
}

// The responses did not change shape in v0.2. They still gain a V2 variant,
// because a method's version is uniform across its request, response and error.

impl IntoLatest for HostPaymentTopUpResponse {
	fn into_latest(self) -> Self::Latest {}
}

impl FromLatest for HostPaymentTopUpResponse {
	fn from_latest((): Self::Latest, target: u8) -> Self {
		if target >= 2 { Self::V2 } else { Self::V1 }
	}
}

impl IntoLatest for HostPaymentTopUpError {
	fn into_latest(self) -> Self::Latest {
		match self {
			Self::V1(error) => match error {
				v01::HostPaymentTopUpError::InvalidSource => {
					v02::HostPaymentTopUpError::InvalidSource
				},
				v01::HostPaymentTopUpError::SourceBusy => v02::HostPaymentTopUpError::SourceBusy,
				v01::HostPaymentTopUpError::InvalidAmount => {
					v02::HostPaymentTopUpError::InvalidAmount
				},
				v01::HostPaymentTopUpError::Unknown { reason } => {
					v02::HostPaymentTopUpError::Unknown { reason }
				},
				v01::HostPaymentTopUpError::AlreadyExists => {
					v02::HostPaymentTopUpError::AlreadyExists
				},
			},
			Self::V2(latest) => latest,
		}
	}
}

impl FromLatest for HostPaymentTopUpError {
	fn from_latest(latest: Self::Latest, target: u8) -> Self {
		if target >= 2 {
			return Self::V2(latest);
		}
		Self::V1(match latest {
			v02::HostPaymentTopUpError::InvalidSource => v01::HostPaymentTopUpError::InvalidSource,
			v02::HostPaymentTopUpError::SourceBusy => v01::HostPaymentTopUpError::SourceBusy,
			v02::HostPaymentTopUpError::InvalidAmount => v01::HostPaymentTopUpError::InvalidAmount,
			v02::HostPaymentTopUpError::Unknown { reason } => {
				v01::HostPaymentTopUpError::Unknown { reason }
			},
			v02::HostPaymentTopUpError::AlreadyExists => v01::HostPaymentTopUpError::AlreadyExists,
			// A v0.1 caller names no funding session, so it never provokes
			// this. The downgrade still has to be total.
			refusal @ v02::HostPaymentTopUpError::FundingNotServed => {
				v01::HostPaymentTopUpError::Unknown { reason: refusal.to_string() }
			},
		})
	}
}

impl IntoLatest for HostPaymentRequest {
	fn into_latest(self) -> Self::Latest {
		match self {
			Self::V1(v01::HostPaymentRequest { from, amount, destination, id }) => {
				v02::HostPaymentRequest { from, amount, destination, id, funding: None }
			},
			Self::V2(latest) => latest,
		}
	}
}

impl IntoLatest for HostPaymentResponse {
	fn into_latest(self) -> Self::Latest {}
}

impl FromLatest for HostPaymentResponse {
	fn from_latest((): Self::Latest, target: u8) -> Self {
		if target >= 2 { Self::V2 } else { Self::V1 }
	}
}

impl IntoLatest for HostPaymentError {
	fn into_latest(self) -> Self::Latest {
		match self {
			Self::V1(error) => match error {
				v01::HostPaymentError::Rejected => v02::HostPaymentError::Rejected,
				v01::HostPaymentError::InsufficientBalance => {
					v02::HostPaymentError::InsufficientBalance
				},
				v01::HostPaymentError::Unknown { reason } => {
					v02::HostPaymentError::Unknown { reason }
				},
				v01::HostPaymentError::InvalidAmount => v02::HostPaymentError::InvalidAmount,
				v01::HostPaymentError::AlreadyExists => v02::HostPaymentError::AlreadyExists,
			},
			Self::V2(latest) => latest,
		}
	}
}

impl FromLatest for HostPaymentError {
	fn from_latest(latest: Self::Latest, target: u8) -> Self {
		if target >= 2 {
			return Self::V2(latest);
		}
		Self::V1(match latest {
			v02::HostPaymentError::Rejected => v01::HostPaymentError::Rejected,
			v02::HostPaymentError::InsufficientBalance => {
				v01::HostPaymentError::InsufficientBalance
			},
			v02::HostPaymentError::Unknown { reason } => v01::HostPaymentError::Unknown { reason },
			v02::HostPaymentError::InvalidAmount => v01::HostPaymentError::InvalidAmount,
			v02::HostPaymentError::AlreadyExists => v01::HostPaymentError::AlreadyExists,
			// A v0.1 caller names no funding session, so it never provokes
			// this. The downgrade still has to be total.
			refusal @ v02::HostPaymentError::FundingNotServed => {
				v01::HostPaymentError::Unknown { reason: refusal.to_string() }
			},
		})
	}
}

#[cfg(test)]
mod tests;
