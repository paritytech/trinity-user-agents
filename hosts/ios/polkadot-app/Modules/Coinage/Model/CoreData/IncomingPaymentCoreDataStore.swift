import AsyncExtensions
import Coinage
import CoreData
import Foundation
import Operation_iOS
import StructuredConcurrency

/// CoreData-backed ``IncomingPaymentStoring``. Records are keyed by their `groupId`
/// (`"top up:productId:paymentId"`) as `identifier`, hold no secrets, and are settled once with a
/// terminal verdict; "active" means `outcomeTag == nil`. The verdict goes through a write-only mapper
/// so settling never fetch-modify-saves the whole record.
final class IncomingPaymentCoreDataStore: IncomingPaymentStoring, @unchecked Sendable {
    private let storageFacade: StorageFacadeProtocol
    private let repository: AnyDataProviderRepository<IncomingPayment>
    private let activeRepository: AnyDataProviderRepository<IncomingPayment>

    init(storageFacade: StorageFacadeProtocol) {
        self.storageFacade = storageFacade

        repository = Self.makeRepository(storageFacade, filter: nil, mapper: IncomingPaymentMapper())
        activeRepository = Self.makeRepository(
            storageFacade, filter: Self.activeFilter, mapper: IncomingPaymentMapper()
        )
    }

    func save(_ payment: IncomingPayment, authorization: @escaping @Sendable () throws -> Void) async throws {
        try await storageFacade.databaseService.perform { context in
            do {
                // This runs on the actual database executor, not before an async save is enqueued.
                try authorization()
                let existing: CDIncomingPayment? = try context.first(
                    for: NSPredicate(format: "identifier == %@", payment.groupId)
                )
                // Registration is insert-only; never overwrite legacy or another wallet's same group.
                guard existing == nil else { throw IncomingPaymentError.alreadyExists }
                let entity = CDIncomingPayment(context: context)
                try IncomingPaymentMapper().populate(entity: entity, from: payment, using: context)
                try context.save()
            } catch {
                context.rollback()
                throw error
            }
        }
    }

    func fetch(groupId: CoinageTxGroupId) async throws -> IncomingPayment? {
        try await repository.fetchOperation(by: { groupId }, options: .init()).asyncExecute()
    }

    func fetchActivePayments() async throws -> [IncomingPayment] {
        try await activeRepository.fetchAllOperation(with: RepositoryFetchOptions()).asyncExecute()
    }

    func settle(
        groupId: CoinageTxGroupId,
        ownerId: Data,
        outcome: IncomingPaymentTerminalOutcome,
        authorization: @escaping @Sendable () throws -> Void
    ) async throws {
        let update = IncomingPaymentOutcomeUpdate(groupId: groupId, outcome: outcome)
        try await storageFacade.databaseService.perform { context in
            do {
                try authorization()
                guard let entity: CDIncomingPayment = try context.first(
                    for: NSPredicate(format: "identifier == %@", groupId)
                ), entity.ownerId == ownerId else {
                    throw IncomingPaymentError.notFound(groupId)
                }
                try IncomingPaymentOutcomeMapper().populate(entity: entity, from: update, using: context)
                try context.save()
            } catch {
                context.rollback()
                throw error
            }
        }
    }

    func observeActivePayments() -> AnyAsyncSequence<[IncomingPayment]> {
        storageFacade.subscribeSnapshot(
            mapper: AnyCoreDataMapper(IncomingPaymentMapper()),
            filter: Self.activeFilter
        )
    }
}

// MARK: - Private

private extension IncomingPaymentCoreDataStore {
    static var activeFilter: NSPredicate {
        NSPredicate(format: "%K == nil", #keyPath(CDIncomingPayment.outcomeTag))
    }

    static func makeRepository<Mapper: CoreDataMapperProtocol>(
        _ storageFacade: StorageFacadeProtocol,
        filter: NSPredicate?,
        mapper: Mapper
    ) -> AnyDataProviderRepository<Mapper.DataProviderModel>
        where Mapper.DataProviderModel: Operation_iOS.Identifiable, Mapper.CoreDataEntity == CDIncomingPayment {
        AnyDataProviderRepository(
            storageFacade.createRepository(filter: filter, sortDescriptors: [], mapper: AnyCoreDataMapper(mapper))
        )
    }
}
