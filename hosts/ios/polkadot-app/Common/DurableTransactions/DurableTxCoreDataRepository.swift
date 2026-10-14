import AsyncExtensions
import CoreData
import DurableTransactions
import Foundation
import Operation_iOS

/// CoreData-backed ``DurableTxRepositoryProtocol``: the engine's ledger row, domain-neutral.
///
/// Registration opens the one write transaction; a domain's store writes its own rows inside it through
/// ``CoreDataRegistrationScope``, so both halves commit or roll back together. The shared serial
/// `databaseService` queue serialises concurrent registrations, which is what makes a domain's invariant
/// checks inside the hook sound. Entries are never deleted.
final class DurableTxCoreDataRepository: DurableTxRepositoryProtocol, @unchecked Sendable {
    private let repository: AnyDataProviderRepository<DurableTxEntry>
    private let storageFacade: StorageFacadeProtocol
    private let databaseService: CoreDataServiceProtocol
    private let rowObservers: [any DurableTxRowObserving]
    private let mapper = DurableTxMapper()

    init(storageFacade: StorageFacadeProtocol, rowObservers: [any DurableTxRowObserving] = []) {
        self.storageFacade = storageFacade
        self.rowObservers = rowObservers
        databaseService = storageFacade.databaseService

        let entryRepository = storageFacade.createRepository(
            filter: nil,
            sortDescriptors: [NSSortDescriptor(key: #keyPath(CDDurableTx.sequence), ascending: true)],
            mapper: AnyCoreDataMapper(DurableTxMapper())
        )
        repository = AnyDataProviderRepository(entryRepository)
    }
}

// MARK: - Registration

extension DurableTxCoreDataRepository {
    func register(
        _ registrations: [DurableTxRegistration],
        onRegister: @escaping DurableTxRegistrationHook
    ) async throws -> [DurableTxId] {
        guard !registrations.isEmpty else { return [] }
        return try await withTransaction { context in
            var ids: [DurableTxId] = []
            for registration in registrations {
                let entry = try registration.makeEntry(id: DurableTxId(), sequence: self.nextSequence(in: context))
                try self.insert(entry, policy: registration.policy, in: context)
                ids.append(entry.id)
            }

            // Inside the transaction, before the rows are visible: the domain writes its rows and the
            // caller takes ownership, so a pass can never reach a committed entry before its watcher.
            try onRegister(CoreDataRegistrationScope(context: context), ids)
            return ids
        }
    }

    func schedule(
        _ schedules: [DurableTxSchedule],
        in scope: (any DurableTxRegistrationScope)?,
        onRegister: @escaping DurableTxRegistrationHook
    ) async throws -> [DurableTxId] {
        guard !schedules.isEmpty else { return [] }

        if let scope {
            return try schedule(schedules, joining: scope, onRegister: onRegister)
        }

        return try await withTransaction { context in
            try self.insertSchedules(schedules, in: context, onRegister: onRegister)
        }
    }

    func schedule(
        _ schedules: [DurableTxSchedule],
        joining scope: any DurableTxRegistrationScope,
        onRegister: DurableTxRegistrationHook
    ) throws -> [DurableTxId] {
        guard let scope = scope as? CoreDataRegistrationScope else {
            throw DurableTxError.foreignRegistrationScope
        }

        guard !schedules.isEmpty else { return [] }

        // No transaction of our own: the caller's is already open on the shared serial writer, and a
        // second one would deadlock on it.
        return try insertSchedules(schedules, in: scope.context, onRegister: onRegister)
    }

    func startAttempt(id: DurableTxId, attempt: DurableTxAttempt) async throws -> Bool {
        try await withTransaction { context in
            guard let entity = try self.entity(id, in: context),
                  entity.status == Int16(DurableTxStatus.pendingSubmission.rawValue)
            else {
                return false
            }

            DurableTxMapper.apply(attempt: attempt, to: entity)
            // A new attempt has observed nothing yet, so whatever the last one recorded goes with it.
            DurableTxMapper.apply(status: .pending, successDetectedAt: nil, to: entity)

            for observer in self.rowObservers {
                observer.didChangeStatus(of: entity, in: context)
            }

            return true
        }
    }

    func abandonSubmission(id: DurableTxId) async throws -> Bool {
        try await withTransaction { context in
            guard let entity = try self.entity(id, in: context),
                  entity.status == Int16(DurableTxStatus.pendingSubmission.rawValue)
            else {
                return false
            }

            DurableTxMapper.apply(status: .failure, successDetectedAt: nil, to: entity)

            for observer in self.rowObservers {
                observer.didChangeStatus(of: entity, in: context)
            }

            return true
        }
    }
}

// MARK: - Status writes

extension DurableTxCoreDataRepository {
    @discardableResult
    func updateTxStatus(
        for id: DurableTxId,
        expectedCurrentStatus: DurableTxStatus,
        expectedTxHash: Data,
        verdict: Verdict
    ) async throws -> Bool {
        try await withTransaction { context in
            guard let entity = try self.entity(id, in: context) else { return false }
            let current = try self.mapper.transform(entity: entity)
            guard current.status.isLive,
                  current.status == expectedCurrentStatus,
                  current.txHash == expectedTxHash
            else {
                return false
            }

            // Skip a write that changes nothing — a verdict restating the current status and record.
            guard current.status != verdict.status || current.successDetectedAt != verdict.successDetectedAt else {
                return false
            }

            DurableTxMapper.apply(status: verdict.status, successDetectedAt: verdict.successDetectedAt, to: entity)
            for observer in self.rowObservers {
                observer.didChangeStatus(of: entity, in: context)
            }
            return true
        }
    }
}

// MARK: - Reads

extension DurableTxCoreDataRepository {
    func getAllEntries() async throws -> [DurableTxEntry] {
        try await repository
            .fetchAllOperation(with: RepositoryFetchOptions())
            .asyncExecute()
            .sorted { $0.sequence < $1.sequence }
    }

    func getAllEntries(domain: TxDomainId) async throws -> [DurableTxEntry] {
        let domainRepository = storageFacade.createRepository(
            filter: NSPredicate(
                format: "%K == %@ AND %K != %d",
                #keyPath(CDDurableTx.domainId),
                domain.rawValue,
                #keyPath(CDDurableTx.status),
                DurableTxStatus.pendingSubmission.rawValue
            ),
            sortDescriptors: [NSSortDescriptor(key: #keyPath(CDDurableTx.sequence), ascending: true)],
            mapper: AnyCoreDataMapper(DurableTxMapper())
        )
        return try await AnyDataProviderRepository(domainRepository)
            .fetchAllOperation(with: RepositoryFetchOptions())
            .asyncExecute()
            .sorted { $0.sequence < $1.sequence }
    }

    func getEntry(id: DurableTxId) async throws -> DurableTxEntry? {
        try await repository
            .fetchOperation(by: { id.uuidString }, options: RepositoryFetchOptions())
            .asyncExecute()
    }

    func subscribeStatus(id: DurableTxId) -> AnyAsyncSequence<DurableTxStatus> {
        storageFacade.subscribeSingle(
            mapper: AnyCoreDataMapper(DurableTxMapper()),
            filter: NSPredicate(format: "%K == %@", #keyPath(CDDurableTx.identifier), id.uuidString)
        )
        .compactMap { $0?.status }
        .eraseToAnyAsyncSequence()
    }

    func getGroupEntries(domain: TxDomainId, groupId: DurableTxGroupId) async throws -> [DurableTxEntry] {
        let groupRepository = storageFacade.createRepository(
            filter: Self.groupPredicate(domain: domain, groupId: groupId),
            sortDescriptors: [NSSortDescriptor(key: #keyPath(CDDurableTx.sequence), ascending: true)],
            mapper: AnyCoreDataMapper(DurableTxMapper())
        )
        return try await AnyDataProviderRepository(groupRepository)
            .fetchAllOperation(with: RepositoryFetchOptions())
            .asyncExecute()
            .sorted { $0.sequence < $1.sequence }
    }

    func subscribeGroupEntries(domain: TxDomainId, groupId: DurableTxGroupId) -> AnyAsyncSequence<[DurableTxEntry]> {
        storageFacade.subscribeSnapshot(
            mapper: AnyCoreDataMapper(DurableTxMapper()),
            filter: Self.groupPredicate(domain: domain, groupId: groupId),
            transform: { $0.sorted { $0.sequence < $1.sequence } }
        )
    }

    @discardableResult
    func abandonSubmissions(domain: TxDomainId, policyId: SubmissionPolicyId) async throws -> Int {
        try await withTransaction { context in
            let request = NSFetchRequest<CDDurableTx>(entityName: "CDDurableTx")
            request.predicate = NSPredicate(
                format: "%K == %@ AND %K == %@ AND %K == %d",
                #keyPath(CDDurableTx.domainId),
                domain.rawValue,
                #keyPath(CDDurableTx.submissionPolicyId),
                policyId.rawValue,
                #keyPath(CDDurableTx.status),
                DurableTxStatus.pendingSubmission.rawValue
            )
            request.returnsObjectsAsFaults = false

            let entities = try context.fetch(request)

            for entity in entities {
                DurableTxMapper.apply(status: .failure, successDetectedAt: nil, to: entity)

                for observer in self.rowObservers {
                    observer.didChangeStatus(of: entity, in: context)
                }
            }

            return entities.count
        }
    }

    func getSubmissionPolicy(id: DurableTxId) async throws -> SubmissionPolicy? {
        try await databaseService.performRead { context in
            try self.entity(id, in: context).flatMap { DurableTxMapper.policy(of: $0) }
        }
    }

    func getPendingSubmissions(
        policyId: SubmissionPolicyId,
        groupId: DurableTxGroupId?
    ) async throws -> [ScheduledDurableTx] {
        try await databaseService.performRead { context in
            try self.pendingSubmissions(in: context, policyId: policyId, groupId: groupId)
        }
    }

    func subscribePendingSubmissions() -> AnyAsyncSequence<[ScheduledDurableTx]> {
        storageFacade.subscribeSnapshot(
            mapper: AnyCoreDataMapper(DurableTxMapper()),
            filter: NSPredicate(
                format: "%K == %d",
                #keyPath(CDDurableTx.status),
                DurableTxStatus.pendingSubmission.rawValue
            ),
            transform: { $0.sorted { $0.sequence < $1.sequence } }
        )
        // The snapshot is the trigger; the rows come from one fetch that reads each policy with its
        // entry, rather than a query per row. A failed read ends the stream instead of reading as
        // "nothing is waiting" — an empty emission launches no bucket, and with no further change to
        // the table there would be no later emission to correct it.
        .map { [weak self] _ -> [ScheduledDurableTx] in
            guard let self else { return [] }

            return try await databaseService.performRead { context in
                try self.pendingSubmissions(in: context)
            }
        }
        .eraseToAnyAsyncSequence()
    }

    private static func groupPredicate(domain: TxDomainId, groupId: DurableTxGroupId) -> NSPredicate {
        NSPredicate(
            format: "%K == %@ AND %K == %@",
            #keyPath(CDDurableTx.domainId),
            domain.rawValue,
            #keyPath(CDDurableTx.groupId),
            groupId
        )
    }
}

// MARK: - Transaction

private extension DurableTxCoreDataRepository {
    /// Executes a transaction block within the shared CoreData context, saving on success and rolling
    /// back on error, so a rejected registration leaves nothing behind — in either store.
    ///
    /// WARNING: Do not call `withTransaction` from within another `withTransaction` body — that would
    /// deadlock on the writer's serial dispatch queue. A domain store handed the registration scope writes
    /// through the scope's context and never opens a transaction of its own.
    func withTransaction<T>(_ body: @escaping (NSManagedObjectContext) throws -> T) async throws -> T {
        try await databaseService.performWrite { context in
            try body(context)
        }
    }
}

// MARK: - Context helpers

private extension DurableTxCoreDataRepository {
    func nextSequence(in context: NSManagedObjectContext) throws -> Int64 {
        let request = NSFetchRequest<CDDurableTx>(entityName: "CDDurableTx")
        request.sortDescriptors = [NSSortDescriptor(key: #keyPath(CDDurableTx.sequence), ascending: false)]
        request.fetchLimit = 1
        request.returnsObjectsAsFaults = false

        let entities = try context.fetch(request)
        return (entities.first?.sequence ?? 0) + 1
    }

    /// The one place a durable row is created.
    ///
    /// The policy is a separate argument because ``DurableTxEntry`` carries none — it belongs to the
    /// registration, and a row's policy is read back on its own. Requiring it here is what stops a
    /// creation path from silently dropping it: an eagerly submitted transaction that loses its policy
    /// has no retry, and nothing downstream can tell that from one that never had a policy.
    func insert(
        _ entry: DurableTxEntry,
        policy: SubmissionPolicy?,
        in context: NSManagedObjectContext
    ) throws {
        let entity = try context.insertNew(CDDurableTx.self)
        try mapper.populate(entity: entity, from: entry, using: context)
        DurableTxMapper.apply(policy: policy, to: entity)
    }

    func insertSchedules(
        _ schedules: [DurableTxSchedule],
        in context: NSManagedObjectContext,
        onRegister: DurableTxRegistrationHook
    ) throws -> [DurableTxId] {
        var ids: [DurableTxId] = []

        for schedule in schedules {
            let entry = try schedule.makeEntry(id: DurableTxId(), sequence: nextSequence(in: context))
            try insert(entry, policy: schedule.policy, in: context)
            ids.append(entry.id)
        }

        try onRegister(CoreDataRegistrationScope(context: context), ids)

        return ids
    }

    /// Every transaction waiting to be built, narrowed in the fetch rather than in memory: a bucket asks
    /// for its own rows twice a round, and the table grows for the life of the installation.
    func pendingSubmissions(
        in context: NSManagedObjectContext,
        policyId: SubmissionPolicyId? = nil,
        groupId: DurableTxGroupId? = nil
    ) throws -> [ScheduledDurableTx] {
        var predicates = [
            NSPredicate(
                format: "%K == %d",
                #keyPath(CDDurableTx.status),
                DurableTxStatus.pendingSubmission.rawValue
            )
        ]

        if let policyId {
            predicates.append(NSPredicate(
                format: "%K == %@",
                #keyPath(CDDurableTx.submissionPolicyId),
                policyId.rawValue
            ))
        }

        if let groupId {
            predicates.append(NSPredicate(format: "%K == %@", #keyPath(CDDurableTx.groupId), groupId))
        } else if policyId != nil {
            // An ungrouped bucket is the rows with no group, not every group's rows.
            predicates.append(NSPredicate(format: "%K == nil", #keyPath(CDDurableTx.groupId)))
        }

        let request = NSFetchRequest<CDDurableTx>(entityName: "CDDurableTx")
        request.predicate = NSCompoundPredicate(andPredicateWithSubpredicates: predicates)
        request.sortDescriptors = [NSSortDescriptor(key: #keyPath(CDDurableTx.sequence), ascending: true)]
        request.returnsObjectsAsFaults = false

        return try context.fetch(request).compactMap { entity in
            guard let identifier = entity.identifier,
                  let id = UUID(uuidString: identifier),
                  let domainId = entity.domainId,
                  let policy = DurableTxMapper.policy(of: entity)
            else {
                return nil
            }

            return ScheduledDurableTx(
                id: id,
                domainId: TxDomainId(domainId),
                groupId: entity.groupId,
                policy: policy
            )
        }
    }

    func entity(_ id: DurableTxId, in context: NSManagedObjectContext) throws -> CDDurableTx? {
        try context.first(
            for: NSPredicate(format: "%K == %@", #keyPath(CDDurableTx.identifier), id.uuidString)
        )
    }
}
