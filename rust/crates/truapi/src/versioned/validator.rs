//! Versioned envelopes for experimental host-custodied validation.

use crate::v01;

truapi_macros::versioned_type! {
	pub enum HostValidatorOpenRequest { V1 => v01::HostValidatorOpenRequest }
	pub enum HostValidatorOpenResponse { V1 => v01::HostValidatorOpenResponse }
	pub enum HostValidatorOpenError { V1 => v01::HostValidatorError }
	pub enum HostValidatorCommitRequest { V1 => v01::HostValidatorCommitRequest }
	pub enum HostValidatorCommitResponse { V1 => v01::HostValidatorCommitResponse }
	pub enum HostValidatorCommitError { V1 => v01::HostValidatorError }
	pub enum HostValidatorSignRequest { V1 => v01::HostValidatorSignRequest }
	pub enum HostValidatorSignResponse { V1 => v01::HostValidatorSignResponse }
	pub enum HostValidatorSignError { V1 => v01::HostValidatorError }
	pub enum HostValidatorCloseRequest { V1 => v01::HostValidatorCloseRequest }
	pub enum HostValidatorCloseResponse { V1 }
	pub enum HostValidatorCloseError { V1 => v01::HostValidatorError }
	pub enum HostValidatorVrfOutputRequest { V1 => v01::HostValidatorVrfOutputRequest }
	pub enum HostValidatorVrfOutputResponse { V1 => v01::HostValidatorVrfOutputResponse }
	pub enum HostValidatorVrfOutputError { V1 => v01::HostValidatorError }
	pub enum HostValidatorTicketProveRequest { V1 => v01::HostValidatorTicketProveRequest }
	pub enum HostValidatorTicketProveResponse { V1 => v01::HostValidatorTicketProveResponse }
	pub enum HostValidatorTicketProveError { V1 => v01::HostValidatorError }
	pub enum HostValidatorEntropySourceRequest { V1 => v01::HostValidatorEntropySourceRequest }
	pub enum HostValidatorEntropySourceResponse { V1 => v01::HostValidatorEntropySourceResponse }
	pub enum HostValidatorEntropySourceError { V1 => v01::HostValidatorError }
	pub enum HostValidatorSealRequest { V1 => v01::HostValidatorSealRequest }
	pub enum HostValidatorSealResponse { V1 => v01::HostValidatorSealResponse }
	pub enum HostValidatorSealError { V1 => v01::HostValidatorError }
	pub enum HostValidatorFullSignRequest { V1 => v01::HostValidatorFullSignRequest }
	pub enum HostValidatorFullSignResponse { V1 => v01::HostValidatorFullSignResponse }
	pub enum HostValidatorFullSignError { V1 => v01::HostValidatorError }
	pub enum HostValidatorAuditProveRequest { V1 => v01::HostValidatorAuditProveRequest }
	pub enum HostValidatorAuditProveResponse { V1 => v01::HostValidatorAuditProveResponse }
	pub enum HostValidatorAuditProveError { V1 => v01::HostValidatorError }
}
