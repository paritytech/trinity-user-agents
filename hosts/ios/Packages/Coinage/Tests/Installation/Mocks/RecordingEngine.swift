import AsyncExtensions
import BigInt
import DurableTransactions
import ExtrinsicService
import Foundation
import Individuality
import KeyDerivation
import os
import SubstrateSdk
@testable import Coinage

/// Records what the submitter hands to the engine.
final class RecordingEngine: DurableTxServicing, @unchecked Sendable {
    private(set) var submissions: [(domain: TxDomainId, requests: Int, groupId: DurableTxGroupId?)] = []
    let submittedId = UUID()

    func submitTransactions(
        domain: TxDomainId,
        requests: [DurableTxRequest],
        groupId: DurableTxGroupId?,
        policies _: [SubmissionPolicy?],
        onRegister _: @escaping DurableTxRegistrationHook
    ) async throws -> [DurableTxId] {
        submissions.append((domain, requests.count, groupId))
        return [submittedId]
    }

    func schedule(
        domain _: TxDomainId,
        groupId _: DurableTxGroupId?,
        policies _: [SubmissionPolicy],
        joining _: any DurableTxRegistrationScope,
        onRegister _: DurableTxRegistrationHook
    ) throws -> [DurableTxId] {
        []
    }

    func subscribeTransactionStatus(_: DurableTxId) -> AnyAsyncSequence<DurableTxStatus> {
        AsyncStream<DurableTxStatus> { $0.finish() }.eraseToAnyAsyncSequence()
    }

    func getGroupEntries(domain _: TxDomainId, groupId _: DurableTxGroupId) async throws -> [DurableTxEntry] { [] }

    func subscribeGroupEntries(
        domain _: TxDomainId,
        groupId _: DurableTxGroupId
    ) -> AnyAsyncSequence<[DurableTxEntry]> {
        AsyncStream<[DurableTxEntry]> { $0.finish() }.eraseToAnyAsyncSequence()
    }

    func startRecoveryPass() {}
    func start() {}
    func stop() {}
}
