import CoreData
import Foundation
import FoundationExt
import Operation_iOS

enum TransferStateStoreError: Error {
    case messageNotFound(Chat.MessageId)
}

/// A changed state also touches the message row: the chat snapshot re-maps only rows the fetched
/// results controller reports, the same trick `CoinageTxRowObserver` uses for coin and voucher rows.
final class TransferStateCoreDataStore: TransferStateStoring, @unchecked Sendable {
    private let databaseService: CoreDataServiceProtocol
    private let dateProvider: any DateProviding

    init(storageFacade: StorageFacadeProtocol, dateProvider: any DateProviding = NowDateProvider()) {
        databaseService = storageFacade.databaseService
        self.dateProvider = dateProvider
    }

    func beginIncoming(messageId: Chat.MessageId) async throws -> Date {
        let now = await dateProvider.read()

        return try await databaseService.performWrite { context in
            let message = try Self.message(messageId, in: context)

            if let row = message.incomingTransferState {
                return try Self.firstAttemptAt(of: row)
            }

            let row = try Self.insertIncomingRow(for: message, firstAttemptAt: now, in: context)
            TransferStateRows.apply(IncomingTransferState(status: .detecting), to: row)
            return now
        }
    }

    func updateIncoming(messageId: Chat.MessageId, state: IncomingTransferState) async throws {
        let now = await dateProvider.read()

        try await databaseService.performWrite { context in
            let message = try Self.message(messageId, in: context)
            let row = try message.incomingTransferState
                ?? Self.insertIncomingRow(for: message, firstAttemptAt: now, in: context)

            guard !TransferStateRows.isUnchanged(row, comparedTo: state) else { return }

            TransferStateRows.apply(state, to: row)
            Self.touch(message, key: #keyPath(CDChatMessage.incomingTransferState))
        }
    }

    func updateOutgoing(messageId: Chat.MessageId, state: OutgoingTransferState) async throws {
        try await databaseService.performWrite { context in
            let message = try Self.message(messageId, in: context)
            let row = try message.outgoingTransferState ?? Self.insertOutgoingRow(for: message, in: context)

            guard !TransferStateRows.isUnchanged(row, comparedTo: state) else { return }

            TransferStateRows.apply(state, to: row)
            Self.touch(message, key: #keyPath(CDChatMessage.outgoingTransferState))
        }
    }
}

private extension TransferStateCoreDataStore {
    static func message(_ messageId: Chat.MessageId, in context: NSManagedObjectContext) throws -> CDChatMessage {
        let predicate = NSPredicate(format: "%K == %@", #keyPath(CDChatMessage.messageId), messageId)
        guard let message: CDChatMessage = try context.first(for: predicate) else {
            throw TransferStateStoreError.messageNotFound(messageId)
        }
        return message
    }

    static func insertIncomingRow(
        for message: CDChatMessage,
        firstAttemptAt: Date,
        in context: NSManagedObjectContext
    ) throws -> CDIncomingTransferState {
        let row = try context.insertNew(CDIncomingTransferState.self)
        row.firstAttemptAt = firstAttemptAt
        message.incomingTransferState = row
        return row
    }

    static func insertOutgoingRow(
        for message: CDChatMessage,
        in context: NSManagedObjectContext
    ) throws -> CDOutgoingTransferState {
        let row = try context.insertNew(CDOutgoingTransferState.self)
        message.outgoingTransferState = row
        return row
    }

    static func firstAttemptAt(of row: CDIncomingTransferState) throws -> Date {
        guard let firstAttemptAt = row.firstAttemptAt else {
            throw CoreDataMapperError.missingRequiredData(keyPath: #keyPath(CDIncomingTransferState.firstAttemptAt))
        }
        return firstAttemptAt
    }

    /// Marks the object updated without changing it, so the save carries it to the observer context.
    static func touch(_ object: NSManagedObject, key: String) {
        object.willChangeValue(forKey: key)
        object.didChangeValue(forKey: key)
    }
}
