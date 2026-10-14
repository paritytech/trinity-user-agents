import Coinage
import CoreData
import Foundation
import Operation_iOS
import StructuredConcurrency
import AsyncExtensions

/// CoreData-backed implementation of ``ExternalPaymentStoring``.
final class ExternalPaymentCoreDataStore: ExternalPaymentStoring, @unchecked Sendable {
    private let repository: AnyDataProviderRepository<ExternalPayment>
    private let stageRepository: AnyDataProviderRepository<ExternalPayment>
    private let storageFacade: StorageFacadeProtocol
    private let operationQueue: OperationQueue
    private let logger: LoggerProtocol

    init(
        storageFacade: StorageFacadeProtocol,
        operationQueue: OperationQueue = OperationManagerFacade.sharedDefaultQueue,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.storageFacade = storageFacade
        self.operationQueue = operationQueue
        self.logger = logger

        let fullRepository = storageFacade.createRepository(
            filter: nil,
            sortDescriptors: [],
            mapper: AnyCoreDataMapper(ExternalPaymentMapper())
        )
        repository = AnyDataProviderRepository(fullRepository)

        let stageRepo = storageFacade.createRepository(
            filter: nil,
            sortDescriptors: [],
            mapper: AnyCoreDataMapper(ExternalPaymentStageMapper())
        )
        stageRepository = AnyDataProviderRepository(stageRepo)
    }

    func save(payment: ExternalPayment) async throws {
        try await repository.saveOperation({ [payment] }, { [] }).asyncExecute()
    }

    func fetchPayment(byId id: String) async throws -> ExternalPayment? {
        try await repository.fetchOperation(
            by: { id },
            options: .init()
        )
        .asyncExecute()
    }

    func observePayment(id: String) -> AnyAsyncSequence<ExternalPayment?> {
        storageFacade.subscribeSingle(
            mapper: AnyCoreDataMapper(ExternalPaymentMapper()),
            filter: NSPredicate(format: "%K == %@", #keyPath(CDExternalPayment.identifier), id)
        )
    }

    func observeNonTerminalPayments() -> AnyAsyncSequence<[ExternalPayment]> {
        let terminalThreshold = Int16(ExternalPayment.Stage.completed.rawValue)

        return makeSnapshotStream(
            filter: NSPredicate(
                format: "%K < %d",
                #keyPath(CDExternalPayment.stage),
                terminalThreshold
            )
        )
    }
}

// MARK: - Private

private extension ExternalPaymentCoreDataStore {
    func makeSnapshotStream(
        filter: NSPredicate
    ) -> AnyAsyncSequence<[ExternalPayment]> {
        storageFacade.subscribeSnapshot(
            mapper: AnyCoreDataMapper(ExternalPaymentMapper()),
            filter: filter
        )
    }
}
