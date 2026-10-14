import AsyncExtensions
import CoreData
import Foundation
import Operation_iOS
import StructuredConcurrency

/// CoreData-backed implementation of ``W3sPaymentHistoryStoring``.
final class W3sPaymentHistoryCoreDataStore: W3sPaymentHistoryStoring, @unchecked Sendable {
    private let repository: AnyDataProviderRepository<W3sPaymentRecord>
    private let statusRepository: AnyDataProviderRepository<W3sPaymentRecord>
    private let storageFacade: StorageFacadeProtocol
    private let operationQueue: OperationQueue
    private let logger: LoggerProtocol
    private let statusSerializer = SerialOperationQueue()

    init(
        storageFacade: StorageFacadeProtocol,
        operationQueue: OperationQueue = OperationManagerFacade.sharedDefaultQueue,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.storageFacade = storageFacade
        self.operationQueue = operationQueue
        self.logger = logger

        let newestFirst = NSSortDescriptor(
            key: #keyPath(CDPaymentRecord.createdAt),
            ascending: false
        )

        let fullRepository = storageFacade.createRepository(
            filter: nil,
            sortDescriptors: [newestFirst],
            mapper: AnyCoreDataMapper(W3sPaymentRecordMapper())
        )
        repository = AnyDataProviderRepository(fullRepository)

        let statusRepo = storageFacade.createRepository(
            filter: nil,
            sortDescriptors: [],
            mapper: AnyCoreDataMapper(W3sPaymentStatusMapper())
        )
        statusRepository = AnyDataProviderRepository(statusRepo)
    }

    func save(_ record: W3sPaymentRecord) async throws {
        try await repository.saveOperation({ [record] }, { [] }).asyncExecute()
    }

    func updateStatus(
        paymentId: String,
        status: W3sPaymentRecord.Status
    ) async throws {
        try await statusSerializer.run { [self] in
            guard let existing = try await fetch(byId: paymentId) else {
                throw W3sPaymentHistoryStoreError.recordNotFound
            }

            guard existing.status.canTransition(to: status) else {
                logger.debug(
                    "W3S \(paymentId): dropped regressive status \(status) over \(existing.status)"
                )
                return
            }

            let updated = existing.updating(status: status)
            try await statusRepository.saveOperation({ [updated] }, { [] }).asyncExecute()
        }
    }

    func fetch(byId id: String) async throws -> W3sPaymentRecord? {
        try await repository.fetchOperation(
            by: { id },
            options: .init()
        )
        .asyncExecute()
    }

    func observeAll() -> AnyAsyncSequence<[W3sPaymentRecord]> {
        storageFacade.subscribeSnapshot(
            mapper: AnyCoreDataMapper(W3sPaymentRecordMapper()),
            filter: nil
        )
        .map { records in
            records.sorted { $0.createdAt > $1.createdAt }
        }
        .eraseToAnyAsyncSequence()
    }

    func observeRecord(paymentId: String) -> AnyAsyncSequence<W3sPaymentRecord?> {
        storageFacade.subscribeSingle(
            mapper: AnyCoreDataMapper(W3sPaymentRecordMapper()),
            filter: .paymentId(paymentId)
        )
    }
}

private enum W3sPaymentHistoryStoreError: Error {
    case recordNotFound
}

private extension NSPredicate {
    static func paymentId(_ value: String) -> NSPredicate {
        NSPredicate(
            format: "%K == %@",
            #keyPath(CDPaymentRecord.paymentId),
            value
        )
    }
}
