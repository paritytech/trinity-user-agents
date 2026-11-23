use derive_more::Display;
use parity_scale_codec::{Decode, Encode};

/// Reasons a worker could not retain its own execution.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Display)]
pub enum WorkerReferenceError {
	/// The connection already holds its maximum number of guest references.
	#[display("too many worker references")]
	TooManyReferences,
}
