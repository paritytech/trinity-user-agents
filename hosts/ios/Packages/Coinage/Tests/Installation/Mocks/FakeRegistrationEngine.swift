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

/// Emits like the store: every write re-emits, identical or not, and a subscriber first sees the
/// current rows.
final class FakeRegistrationEngine: DurableTxServicing, @unchecked Sendable {
    private struct State {
        var groups: [DurableTxGroupId: [DurableTxEntry]] = [:]
        var observers: [DurableTxGroupId: [AsyncStream<[DurableTxEntry]>.Continuation]] = [:]
        var recoveryStarts = 0
    }

    private let state = OSAllocatedUnfairLock(initialState: State())

    var recoveryStarts: Int { state.withLock { $0.recoveryStarts } }

    func current(_ group: DurableTxGroupId) -> [DurableTxEntry] {
        state.withLock { $0.groups[group] ?? [] }
    }

    func set(_ entries: [DurableTxEntry], group: DurableTxGroupId) {
        let observers = state.withLock { state -> [AsyncStream<[DurableTxEntry]>.Continuation] in
            state.groups[group] = entries
            return state.observers[group] ?? []
        }
        observers.forEach { $0.yield(entries) }
    }

    func reEmit(group: DurableTxGroupId) {
        set(current(group), group: group)
    }

    func submitTransactions(
        domain _: TxDomainId,
        requests _: [DurableTxRequest],
        groupId _: DurableTxGroupId?,
        policies _: [SubmissionPolicy?],
        onRegister _: @escaping DurableTxRegistrationHook
    ) async throws -> [DurableTxId] {
        fatalError("the registrar submits through its submitter")
    }

    func schedule(
        domain _: TxDomainId,
        groupId _: DurableTxGroupId?,
        policies _: [SubmissionPolicy],
        joining _: any DurableTxRegistrationScope,
        onRegister _: DurableTxRegistrationHook
    ) throws -> [DurableTxId] {
        fatalError("installation registration never schedules")
    }

    func subscribeTransactionStatus(_: DurableTxId) -> AnyAsyncSequence<DurableTxStatus> {
        AsyncStream<DurableTxStatus> { $0.finish() }.eraseToAnyAsyncSequence()
    }

    func getGroupEntries(domain _: TxDomainId, groupId: DurableTxGroupId) async throws -> [DurableTxEntry] {
        current(groupId)
    }

    func subscribeGroupEntries(domain: TxDomainId, groupId: DurableTxGroupId) -> AnyAsyncSequence<[DurableTxEntry]> {
        precondition(domain == .coinageInstallation)
        return AsyncStream<[DurableTxEntry]> { continuation in
            let snapshot = state.withLock { state -> [DurableTxEntry] in
                state.observers[groupId, default: []].append(continuation)
                return state.groups[groupId] ?? []
            }
            continuation.yield(snapshot)
        }
        .eraseToAnyAsyncSequence()
    }

    func startRecoveryPass() {
        state.withLock { $0.recoveryStarts += 1 }
    }

    func start() {}

    func stop() {}
}
