//! Protocol fixture for generated host-initiated requests.

/// Canonical context and framework failures used by the protocol.
pub use core_runtime::{CallContext, CallError};

/// The original request payload.
pub mod v01 {
	use parity_scale_codec::{Decode, Encode};

	/// Text submitted by the host.
	#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
	pub struct ProductProbeRequest {
		/// Text to return or reject.
		pub text: String,
	}

	/// The product's accepted text.
	#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
	pub struct ProductProbeResponse {
		/// Text after processing.
		pub text: String,
	}

	/// A product's declared rejection.
	#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
	pub enum ProductProbeError {
		/// The text cannot be accepted.
		Rejected {
			/// Product-supplied explanation.
			reason: String,
		},
	}
}

/// Requests with a selectable result for the test peer.
pub mod v02 {
	use parity_scale_codec::{Decode, Encode};

	/// Selects the result the product returns.
	#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
	pub struct ProductProbeRequest {
		/// Text to return or reject.
		pub text: String,
		/// Success, domain rejection, thrown failure, or denial.
		pub outcome: u8,
	}
}

/// Version envelopes consumed by the generator.
pub mod versioned {
	/// Request, response and error envelopes for the probe.
	pub mod probe {
		use parity_scale_codec::{Decode, Encode};

		/// Selects a request shape.
		#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
		pub enum ProductProbeRequest {
			/// Original request.
			V1(crate::v01::ProductProbeRequest),
			/// Request including the result selector.
			V2(crate::v02::ProductProbeRequest),
		}

		/// Selects a response shape.
		#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
		pub enum ProductProbeResponse {
			/// Original response.
			V1(crate::v01::ProductProbeResponse),
			/// Current response.
			V2(crate::v01::ProductProbeResponse),
		}

		/// Keeps the original request shape available to a second method.
		#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
		pub enum ProductProbeLegacyResponse {
			/// Original response.
			V1(crate::v01::ProductProbeResponse),
		}

		/// Selects a declared error shape.
		#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
		pub enum ProductProbeError {
			/// Original rejection.
			V1(crate::v01::ProductProbeError),
			/// Current rejection.
			V2(crate::v01::ProductProbeError),
		}
	}
}

/// The independent protocol surface read from rustdoc.
pub mod api {
	use crate::versioned::probe::{
		ProductProbeError, ProductProbeLegacyResponse, ProductProbeRequest, ProductProbeResponse,
	};
	use core_runtime::{CallContext, CallError, wire, wire_trait};

	/// Host-initiated requests with asynchronous answers.
	#[wire_trait(id = 200)]
	#[async_trait::async_trait]
	pub trait Probe: Send + Sync {
		/// Answer with accepted text or a declared failure.
		///
		/// ```ts
		/// truapi.probe.onAnswer(({ text }) => ok({ text }));
		/// ```
		#[wire(host_initiated, id = 7)]
		async fn answer(
			&self,
			_cx: &CallContext,
			_request: ProductProbeRequest,
		) -> Result<ProductProbeResponse, CallError<ProductProbeError>> {
			Err(CallError::unavailable())
		}

		/// Shares the request envelope while selecting its original payload.
		///
		/// ```ts
		/// truapi.probe.onLegacyAnswer(({ text }) => ok({ text }));
		/// ```
		#[wire(host_initiated, id = 8)]
		async fn legacy_answer(
			&self,
			_cx: &CallContext,
			_request: ProductProbeRequest,
		) -> Result<ProductProbeLegacyResponse, CallError<ProductProbeError>> {
			Err(CallError::unavailable())
		}
	}

	/// Public traits whose callers are generated.
	pub trait TrUApi: Probe {}
}

/// The generated caller compiled against the production request manager.
#[cfg(feature = "runtime")]
#[path = "generated_caller.rs"]
pub mod caller;
