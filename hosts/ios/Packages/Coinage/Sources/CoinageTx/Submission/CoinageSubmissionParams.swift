import DurableTransactions
import Foundation
import FoundationExt
import SubstrateSdk

/// What a transfer's policy needs beyond the ledger.
///
/// The transfer gives up once `buildUntil` has passed with its inputs gone from the chain; an attempt
/// proven unable to land is built again only when `retryFailures`.
public struct TransferSubmissionParams: Equatable, Sendable {
    public let buildUntil: Date
    public let retryFailures: Bool

    public init(buildUntil: Date, retryFailures: Bool) {
        self.buildUntil = buildUntil
        self.retryFailures = retryFailures
    }
}

/// What a claim's policy needs beyond the ledger: the peer's key, which only the payment message
/// carries, so the ledger alone could never rebuild the claim.
public struct ClaimSubmissionParams: Equatable, Sendable {
    public let retryUntil: Date
    public let receivedKey: Data

    public init(retryUntil: Date, receivedKey: Data) {
        self.retryUntil = retryUntil
        self.receivedKey = receivedKey
    }
}

/// The persisted shape of the coinage policies' parameters.
///
/// Stored with every scheduled transaction and opaque to the engine, so a change to either shape needs
/// a versioned decoder for the rows already written.
public enum CoinageSubmissionParams {
    public static let splitPolicyId = SubmissionPolicyId("coinage-split")
    public static let unloadPolicyId = SubmissionPolicyId("coinage-unload")
    public static let claimPolicyId = SubmissionPolicyId("coinage-claim")

    public static func splitPolicy(_ params: TransferSubmissionParams) throws -> SubmissionPolicy {
        try SubmissionPolicy(id: splitPolicyId, params: encode(params))
    }

    public static func unloadPolicy(_ params: TransferSubmissionParams) throws -> SubmissionPolicy {
        try SubmissionPolicy(id: unloadPolicyId, params: encode(params))
    }

    public static func claimPolicy(_ params: ClaimSubmissionParams) throws -> SubmissionPolicy {
        try SubmissionPolicy(id: claimPolicyId, params: encode(params))
    }

    public static func decodeTransfer(_ params: Data) throws -> TransferSubmissionParams {
        let scale = try TransferParamsScale(scaleDecoder: ScaleDecoder(data: params))

        return TransferSubmissionParams(
            buildUntil: Date(timeIntervalSince1970: scale.buildUntilMillis.millisecondsToSeconds()),
            retryFailures: scale.retryFailures
        )
    }

    public static func decodeClaim(_ params: Data) throws -> ClaimSubmissionParams {
        let scale = try ClaimParamsScale(scaleDecoder: ScaleDecoder(data: params))

        return ClaimSubmissionParams(
            retryUntil: Date(timeIntervalSince1970: scale.retryUntilMillis.millisecondsToSeconds()),
            receivedKey: scale.receivedKey
        )
    }
}

public extension TransferSubmissionParams {
    /// A transfer that keeps being rebuilt for as long as the recipient keeps trying to claim.
    ///
    /// The same window ``CoinageConstants/claimRetryWindow`` gives the claim, so neither side gives up
    /// while the other still tries.
    static func retriedTransfer(from start: Date) -> TransferSubmissionParams {
        TransferSubmissionParams(
            buildUntil: start.addingTimeInterval(CoinageConstants.claimRetryWindow),
            retryFailures: true
        )
    }
}

/// Whether an attempt that failed with `failure` is worth building again while `now` is before
/// `deadline`.
///
/// An attempt that simply never got included may land if built again, however late. One that was
/// dispatched and failed, or refused outright, would most likely fail the same way — so it is only
/// retried while the window is still open, which is what keeps a failure that always repeats from being
/// rebuilt for ever.
func retryableFailure(_ failure: DurableFailureKind, now: Date, deadline: Date) -> Bool {
    failure == .expired || now < deadline
}

// MARK: - Persisted shapes

/// Deadlines are stored as whole milliseconds since 1970, so a window opened on one launch means the
/// same thing on the next: an integer round-trips exactly where a `Double` would drift. Clamping
/// rather than trapping is safe because every deadline here is `now` plus a window, so a value below
/// the epoch is unreachable — and a clamp keeps a corrupt row from crashing the builder.
private extension CoinageSubmissionParams {
    static func encode(_ params: TransferSubmissionParams) throws -> Data {
        try TransferParamsScale(
            buildUntilMillis: UInt64(clamping: params.buildUntil.timeIntervalSince1970.milliseconds),
            retryFailures: params.retryFailures
        ).scaleEncoded()
    }

    static func encode(_ params: ClaimSubmissionParams) throws -> Data {
        try ClaimParamsScale(
            retryUntilMillis: UInt64(clamping: params.retryUntil.timeIntervalSince1970.milliseconds),
            receivedKey: params.receivedKey
        ).scaleEncoded()
    }
}

private struct TransferParamsScale: ScaleCodable {
    let buildUntilMillis: UInt64
    let retryFailures: Bool

    init(buildUntilMillis: UInt64, retryFailures: Bool) {
        self.buildUntilMillis = buildUntilMillis
        self.retryFailures = retryFailures
    }

    init(scaleDecoder: any ScaleDecoding) throws {
        buildUntilMillis = try UInt64(scaleDecoder: scaleDecoder)
        retryFailures = try Bool(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try buildUntilMillis.encode(scaleEncoder: scaleEncoder)
        try retryFailures.encode(scaleEncoder: scaleEncoder)
    }
}

/// `receivedKey` is a coin's 64-byte secret, written length-prefixed rather than as raw bytes: a fixed
/// width here silently truncates the key, and a claim that cannot derive its source key is one the policy
/// gives up on.
private struct ClaimParamsScale: ScaleCodable {
    let retryUntilMillis: UInt64
    let receivedKey: Data

    init(retryUntilMillis: UInt64, receivedKey: Data) {
        self.retryUntilMillis = retryUntilMillis
        self.receivedKey = receivedKey
    }

    init(scaleDecoder: any ScaleDecoding) throws {
        retryUntilMillis = try UInt64(scaleDecoder: scaleDecoder)
        receivedKey = try Data(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try retryUntilMillis.encode(scaleEncoder: scaleEncoder)
        try receivedKey.encode(scaleEncoder: scaleEncoder)
    }
}
