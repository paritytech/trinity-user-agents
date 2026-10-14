import DurableTransactions
import Foundation
import Testing
@testable import Coinage

/// The policy parameters are written into the ledger and read back on a later launch, so what matters is
/// that they round-trip exactly and that a corrupt row cannot crash the builder.
@Suite("Coinage Submission Params")
struct CoinageSubmissionParamsTests {
    private let receivedKey = Data(repeating: 0x5A, count: 32)

    @Test("a transfer's params round-trip")
    func transferRoundTrips() throws {
        let params = TransferSubmissionParams(
            buildUntil: Date(timeIntervalSince1970: 1_700_000_000),
            retryFailures: true
        )

        let decoded = try CoinageSubmissionParams.decodeTransfer(
            CoinageSubmissionParams.splitPolicy(params).params
        )

        #expect(decoded == params)
    }

    @Test("retryFailures survives both ways", arguments: [true, false])
    func retryFlagRoundTrips(_ retryFailures: Bool) throws {
        let params = TransferSubmissionParams(
            buildUntil: Date(timeIntervalSince1970: 1_700_000_000),
            retryFailures: retryFailures
        )

        let decoded = try CoinageSubmissionParams.decodeTransfer(
            CoinageSubmissionParams.unloadPolicy(params).params
        )

        #expect(decoded.retryFailures == retryFailures)
    }

    @Test("a claim's params round-trip, key included")
    func claimRoundTrips() throws {
        let params = ClaimSubmissionParams(
            retryUntil: Date(timeIntervalSince1970: 1_700_000_000),
            receivedKey: receivedKey
        )

        let decoded = try CoinageSubmissionParams.decodeClaim(
            CoinageSubmissionParams.claimPolicy(params).params
        )

        #expect(decoded == params)
        #expect(decoded.receivedKey == receivedKey)
    }

    @Test("the claim key round-trips at the size a coin secret actually is")
    func claimKeyRoundTripsAtSecretSize() throws {
        // A coin's secret is 64 bytes — the size `SNKeyFactory.createPublicKey(fromSecret:)` accepts.
        // Truncating it here would make every claim rebuild fail to derive its source key.
        let secret = Data((0 ..< 64).map { UInt8($0) })
        let params = ClaimSubmissionParams(
            retryUntil: Date(timeIntervalSince1970: 1_700_000_000),
            receivedKey: secret
        )

        let decoded = try CoinageSubmissionParams.decodeClaim(
            CoinageSubmissionParams.claimPolicy(params).params
        )

        #expect(decoded.receivedKey == secret)
    }

    @Test("a deadline keeps its milliseconds and loses only what is below them")
    func deadlineKeepsMilliseconds() throws {
        // A sub-millisecond tail is the only thing a whole-millisecond encoding may drop.
        let params = TransferSubmissionParams(
            buildUntil: Date(timeIntervalSince1970: 1_700_000_000.1234),
            retryFailures: true
        )

        let decoded = try CoinageSubmissionParams.decodeTransfer(
            CoinageSubmissionParams.splitPolicy(params).params
        )

        #expect(decoded.buildUntil.timeIntervalSince1970 == 1_700_000_000.123)
    }

    @Test("each policy keeps its own id")
    func policyIdsAreDistinct() throws {
        let transfer = TransferSubmissionParams(buildUntil: .now, retryFailures: true)
        let claim = ClaimSubmissionParams(retryUntil: .now, receivedKey: receivedKey)

        #expect(try CoinageSubmissionParams.splitPolicy(transfer).id == CoinageSubmissionParams.splitPolicyId)
        #expect(try CoinageSubmissionParams.unloadPolicy(transfer).id == CoinageSubmissionParams.unloadPolicyId)
        #expect(try CoinageSubmissionParams.claimPolicy(claim).id == CoinageSubmissionParams.claimPolicyId)
    }

    @Test("split and unload share one encoding, so a row of either reads back the same")
    func transferEncodingIsShared() throws {
        let params = TransferSubmissionParams(
            buildUntil: Date(timeIntervalSince1970: 1_700_000_000),
            retryFailures: false
        )

        let split = try CoinageSubmissionParams.splitPolicy(params).params
        let unload = try CoinageSubmissionParams.unloadPolicy(params).params

        #expect(split == unload)
    }

    @Test("params that cannot be read throw rather than yielding a wrong deadline")
    func corruptParamsThrow() {
        #expect(throws: (any Error).self) {
            try CoinageSubmissionParams.decodeTransfer(Data([0x01]))
        }
        #expect(throws: (any Error).self) {
            try CoinageSubmissionParams.decodeClaim(Data([0x01, 0x02]))
        }
    }

    @Test("a retried transfer's window is the claim's, so neither side gives up first")
    func retriedTransferMatchesClaimWindow() {
        let start = Date(timeIntervalSince1970: 1_700_000_000)

        let params = TransferSubmissionParams.retriedTransfer(from: start)

        #expect(params.retryFailures)
        #expect(params.buildUntil == start.addingTimeInterval(CoinageConstants.claimRetryWindow))
    }

    @Test("an attempt that never landed is always worth rebuilding, however late")
    func expiredIsRetriedPastTheDeadline() {
        let deadline = Date(timeIntervalSince1970: 1_700_000_000)
        let afterwards = deadline.addingTimeInterval(1)

        #expect(retryableFailure(.expired, now: afterwards, deadline: deadline))
    }

    @Test(
        "a failure that would repeat is retried only while the window is open",
        arguments: [DurableFailureKind.dispatchFailed, .rejected]
    )
    func repeatableFailuresAreBounded(_ failure: DurableFailureKind) {
        let deadline = Date(timeIntervalSince1970: 1_700_000_000)

        #expect(retryableFailure(failure, now: deadline.addingTimeInterval(-1), deadline: deadline))
        #expect(!retryableFailure(failure, now: deadline, deadline: deadline))
        #expect(!retryableFailure(failure, now: deadline.addingTimeInterval(1), deadline: deadline))
    }
}
