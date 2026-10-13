import Foundation
import IssueMonitoring
import MessageExchangeKit
import os
import StatementStore
import SubstrateSdk
@testable import polkadot_app

final class RecordingOutgoingChatRequestService: OutgoingChatRequestServicing, @unchecked Sendable {
    /// Failures thrown by the leading sends; later sends succeed.
    var failures: [Error] = []
    var encodedSize = 1_000
    var storedSigners: Set<AccountId> = []
    private(set) var sentSigners: [AccountId] = []

    func send(
        message _: Chat.RequestMessage,
        to _: MessageExchange.Peer,
        ownKeyId _: MessageExchange.Own,
        signer: StatementStoreSigning
    ) async throws {
        if !failures.isEmpty { throw failures.removeFirst() }
        sentSigners.append(signer.accountId)
    }

    func encodedSize(
        of _: Chat.RequestMessage,
        to _: MessageExchange.Peer,
        ownKeyId _: MessageExchange.Own
    ) throws -> Int {
        encodedSize
    }

    func isStored(
        to _: MessageExchange.Peer,
        ownKeyId _: MessageExchange.Own,
        signedBy accountId: AccountId
    ) async throws -> Bool {
        storedSigners.contains(accountId)
    }
}

final class InMemoryChatRequestRenewalStore: ChatRequestRenewalStoring, @unchecked Sendable {
    var candidates: [ChatRequestRenewalCandidate] = []
    private(set) var periodUpdates: [(requestId: String, period: UInt32)] = []

    func renewalCandidates() async throws -> [ChatRequestRenewalCandidate] {
        candidates
    }

    func updateAnonymousPeriod(requestId: String, period: UInt32) async throws {
        periodUpdates.append((requestId, period))
    }
}

final class StubDeliveryAccountResolver: ChatRequestDeliveryAccountResolving, @unchecked Sendable {
    let signer: ChatRequestDeliverySigner
    var statementSize = 10 * 1_024
    private(set) var resolvedCount = 0

    init(signer: ChatRequestDeliverySigner) {
        self.signer = signer
    }

    func maxStatementSize() async throws -> Int {
        statementSize
    }

    func resolveFirstDelivery(requestId _: String) async throws -> ChatRequestDeliverySigner {
        resolvedCount += 1
        return signer
    }
}

final class InMemoryChatRequestDeliveryStore: ChatRequestDeliveryStoring, @unchecked Sendable {
    var isAwaiting = true
    /// Stops awaiting after this many reads; nil keeps `isAwaiting` as is.
    var awaitingReadsLeft: Int?
    private(set) var delivered: [(requestId: String, anonymousPeriod: UInt32)] = []
    private(set) var failed: [String] = []

    func isAwaitingDelivery(requestId _: String) async throws -> Bool {
        if let left = awaitingReadsLeft {
            awaitingReadsLeft = left - 1
            return left > 0
        }
        return isAwaiting
    }

    func markDelivered(requestId: String, anonymousPeriod: UInt32) async throws {
        delivered.append((requestId, anonymousPeriod))
        isAwaiting = false
    }

    func markFailed(requestId: String) async throws {
        failed.append(requestId)
        isAwaiting = false
    }
}

extension ChatRequestDiagnostics {
    static var noop: Self {
        ChatRequestDiagnostics(logger: MockLogger(), issues: .make(reporter: NoopIssueReporter()))
    }
}
