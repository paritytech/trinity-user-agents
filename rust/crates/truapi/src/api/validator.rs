//! Experimental host-custodied JAM validator service.

use crate::versioned::validator::*;
use crate::{CallContext, CallError, wire, wire_trait};

/// Host custody only: consensus, replay, target validity and broadcast remain in
/// the guest. Acquiring custody requires a distinct execution/account-scoped
/// Validator permission and trusted artifact consent. Keys never cross this API.
#[wire_trait(id = 112)]
#[crate::async_trait]
pub trait Validator: Send + Sync {
    /// Acquire an enrolled identity, fetch bounded recovery material, or acknowledge replay.
    ///
    /// ```ts
    /// const result = await truapi.validator.open({
    ///   tag: "Acquire",
    ///   value: { genesis: "0x0000000000000000000000000000000000000000000000000000000000000000" },
    /// });
    /// console.log("custody acquired:", result.isOk());
    /// ```
    #[wire(id = 0)]
    async fn open(
        &self,
        _cx: &CallContext,
        _request: HostValidatorOpenRequest,
    ) -> Result<HostValidatorOpenResponse, CallError<HostValidatorOpenError>> {
        Err(CallError::Unsupported)
    }

    /// Upload complete recovery material or compare-and-commit a durable checkpoint.
    ///
    /// ```ts
    /// // Use the opaque handle returned by open, after uploading complete recovery material.
    /// const result = await truapi.validator.commit({ tag: "Finish", value: { handle: "acquired-handle" } });
    /// console.log("recovery durably stored:", result.isOk());
    /// ```
    #[wire(id = 1)]
    async fn commit(
        &self,
        _cx: &CallContext,
        _request: HostValidatorCommitRequest,
    ) -> Result<HostValidatorCommitResponse, CallError<HostValidatorCommitError>> {
        Err(CallError::Unsupported)
    }

    /// Durably guard and sign one typed GRANDPA domain, never arbitrary bytes.
    ///
    /// ```ts
    /// // Substitute the acquired head and an already durably uploaded, replayable recovery blob.
    /// const zero = "0x0000000000000000000000000000000000000000000000000000000000000000";
    /// const result = await truapi.validator.sign({
    ///   handle: "acquired-handle", revision: "1", head: zero,
    ///   duty: { stage: 0, set: 0, round: "1", target: zero, slot: 0 },
    ///   recovery: { hash: zero, length: 1 },
    /// });
    /// console.log("durable signature available:", result.isOk());
    /// ```
    #[wire(id = 2)]
    async fn sign(
        &self,
        _cx: &CallContext,
        _request: HostValidatorSignRequest,
    ) -> Result<HostValidatorSignResponse, CallError<HostValidatorSignError>> {
        Err(CallError::Unsupported)
    }

    /// Stop admission, drain accepted work and peer transport, then release custody.
    ///
    /// ```ts
    /// const result = await truapi.validator.close({ handle: "acquired-handle" });
    /// console.log("custody drained:", result.isOk());
    /// ```
    #[wire(id = 3)]
    async fn close(
        &self,
        _cx: &CallContext,
        _request: HostValidatorCloseRequest,
    ) -> Result<HostValidatorCloseResponse, CallError<HostValidatorCloseError>> {
        Err(CallError::Unsupported)
    }

    /// Return deterministic Safrole eligibility output under authoring consent.
    ///
    /// ```ts
    /// // Use the acquired handle and canonical parameters of the enrolled chain.
    /// const zero = "0x0000000000000000000000000000000000000000000000000000000000000000";
    /// const result = await truapi.validator.vrfOutput({
    ///   handle: "acquired-handle", parameters: "0x",
    ///   input: { tag: "FallbackSeal", value: { entropy: zero } },
    /// });
    /// console.log("eligibility output available:", result.isOk());
    /// ```
    #[wire(id = 4)]
    async fn vrf_output(
        &self,
        _cx: &CallContext,
        _request: HostValidatorVrfOutputRequest,
    ) -> Result<HostValidatorVrfOutputResponse, CallError<HostValidatorVrfOutputError>> {
        Err(CallError::Unsupported)
    }

    /// Prove one bounded ticket attempt against the ordered waiting ring.
    ///
    /// ```ts
    /// // Supply the consensus-selected waiting ring, enrolled parameters and local index.
    /// const zero = "0x0000000000000000000000000000000000000000000000000000000000000000";
    /// const result = await truapi.validator.ticketProve({
    ///   handle: "acquired-handle", parameters: "0x", ring: [zero],
    ///   index: 0, entropy: zero, attempt: 0,
    /// });
    /// console.log("ticket proof available:", result.isOk());
    /// ```
    #[wire(id = 5)]
    async fn ticket_prove(
        &self,
        _cx: &CallContext,
        _request: HostValidatorTicketProveRequest,
    ) -> Result<HostValidatorTicketProveResponse, CallError<HostValidatorTicketProveError>> {
        Err(CallError::Unsupported)
    }

    /// Sign only the typed JAM entropy-source domain.
    ///
    /// ```ts
    /// // Supply the enrolled parameters and the guest-selected entropy-source ticket id.
    /// const zero = "0x0000000000000000000000000000000000000000000000000000000000000000";
    /// const result = await truapi.validator.entropySource({
    ///   handle: "acquired-handle", parameters: "0x", ticketId: zero,
    /// });
    /// console.log("entropy signature available:", result.isOk());
    /// ```
    #[wire(id = 6)]
    async fn entropy_source(
        &self,
        _cx: &CallContext,
        _request: HostValidatorEntropySourceRequest,
    ) -> Result<HostValidatorEntropySourceResponse, CallError<HostValidatorEntropySourceError>> {
        Err(CallError::Unsupported)
    }

    /// Persist a canonical block-seal intent before signing, one payload per slot.
    ///
    /// ```ts
    /// // Use the acquired head, durably uploaded recovery and canonical encoded SealPayload.
    /// const zero = "0x0000000000000000000000000000000000000000000000000000000000000000";
    /// const result = await truapi.validator.seal({
    ///   handle: "acquired-handle", parameters: "0x", revision: "1", head: zero,
    ///   input: { tag: "FallbackSeal", value: { entropy: zero } },
    ///   recovery: { hash: zero, length: 1 }, payload: "0x",
    /// });
    /// console.log("durable seal available:", result.isOk());
    /// ```
    #[wire(id = 7)]
    async fn seal(
        &self,
        _cx: &CallContext,
        _request: HostValidatorSealRequest,
    ) -> Result<HostValidatorSealResponse, CallError<HostValidatorSealError>> {
        Err(CallError::Unsupported)
    }

    /// Durably guard and sign a typed full-validator domain under explicit role-4 consent.
    ///
    /// ```ts
    /// // Use an explicitly authorized full-validator handle and uploaded recovery.
    /// const zero = "0x0000000000000000000000000000000000000000000000000000000000000000";
    /// const result = await truapi.validator.fullSign({
    ///   handle: "acquired-handle", parameters: "0x", revision: "1", head: zero,
    ///   duty: { tag: "Guarantee", value: { report: zero } },
    ///   recovery: { hash: zero, length: 1 },
    /// });
    /// console.log("durable full-duty signature available:", result.isOk());
    /// ```
    #[wire(id = 8)]
    async fn full_sign(
        &self,
        _cx: &CallContext,
        _request: HostValidatorFullSignRequest,
    ) -> Result<HostValidatorFullSignResponse, CallError<HostValidatorFullSignError>> {
        Err(CallError::Unsupported)
    }

    /// Prove a typed initial or subsequent audit selection under role-4 consent.
    ///
    /// ```ts
    /// const zero = "0x0000000000000000000000000000000000000000000000000000000000000000";
    /// const result = await truapi.validator.auditProve({
    ///   handle: "acquired-handle", parameters: "0x",
    ///   input: { tag: "Initial", value: { entropy: zero } },
    /// });
    /// console.log("audit selection proof available:", result.isOk());
    /// ```
    #[wire(id = 9)]
    async fn audit_prove(
        &self,
        _cx: &CallContext,
        _request: HostValidatorAuditProveRequest,
    ) -> Result<HostValidatorAuditProveResponse, CallError<HostValidatorAuditProveError>> {
        Err(CallError::Unsupported)
    }
}
