import AsyncExtensions
import DurableTransactions

/// Schedules and reads through an ``InMemoryDurableTxRepository`` so a registration commits or rolls back
/// as it does in production; it never submits or runs a pass.
public final class InMemorySchedulingEngine: DurableTxServicing, @unchecked Sendable {
    public enum Failure: Error {
        case submissionUnsupported
    }

    private let store: InMemoryDurableTxRepository

    public init(store: InMemoryDurableTxRepository) {
        self.store = store
    }

    public func submitTransactions(
        domain _: TxDomainId,
        requests _: [DurableTxRequest],
        groupId _: DurableTxGroupId?,
        policies _: [SubmissionPolicy?],
        onRegister _: @escaping DurableTxRegistrationHook
    ) async throws -> [DurableTxId] {
        throw Failure.submissionUnsupported
    }

    public func schedule(
        domain: TxDomainId,
        groupId: DurableTxGroupId?,
        policies: [SubmissionPolicy],
        joining scope: any DurableTxRegistrationScope,
        onRegister: DurableTxRegistrationHook
    ) throws -> [DurableTxId] {
        let schedules = policies.map { DurableTxSchedule(domainId: domain, groupId: groupId, policy: $0) }
        return try store.schedule(schedules, joining: scope, onRegister: onRegister)
    }

    public func schedule(
        domain: TxDomainId,
        groupId: DurableTxGroupId?,
        policies: [SubmissionPolicy],
        onRegister: @escaping DurableTxRegistrationHook
    ) async throws -> [DurableTxId] {
        let schedules = policies.map { DurableTxSchedule(domainId: domain, groupId: groupId, policy: $0) }
        return try await store.schedule(schedules, in: nil, onRegister: onRegister)
    }

    public func subscribeTransactionStatus(_ id: DurableTxId) -> AnyAsyncSequence<DurableTxStatus> {
        store.subscribeStatus(id: id)
    }

    public func getGroupEntries(domain: TxDomainId, groupId: DurableTxGroupId) async throws -> [DurableTxEntry] {
        try await store.getGroupEntries(domain: domain, groupId: groupId)
    }

    public func subscribeGroupEntries(
        domain: TxDomainId,
        groupId: DurableTxGroupId
    ) -> AnyAsyncSequence<[DurableTxEntry]> {
        store.subscribeGroupEntries(domain: domain, groupId: groupId)
    }

    public func startRecoveryPass() {}

    public func start() {}

    public func stop() {}
}
