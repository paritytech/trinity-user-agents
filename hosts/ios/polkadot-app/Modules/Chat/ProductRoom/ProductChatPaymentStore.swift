import BigInt
import Coinage
import CoreData
import DurableTransactions
import Foundation
import FoundationExt
import Operation_iOS
import SubstrateSdk

/// A payment in a room of the chat product, as the host keeps it. The product carries the coin keys over its
/// sessions; the host keeps them here, as the native chat keeps them in its message, so that the transfer monitor
/// follows and claims the coins under the id of the message, and the room draws the state of the transfer.
struct ProductChatPayment: Equatable, Sendable {
    /// How far a payment of the user went to the product. A payment of a contact is always `delivered`.
    enum Delivery: Int16, Sendable {
        /// The coins are handed off: the host sends `SendPayment` until the product has the payment.
        case sending = 0
        case delivered = 1
        /// The product refused the payment. It does not go again, as a native message that failed for good.
        case refused = 2
    }

    let messageId: Chat.MessageId
    let productId: String
    let roomId: String
    let incoming: Bool
    /// Milliseconds since the Unix epoch.
    let timestamp: UInt64
    let totalValue: Balance
    /// The 64-byte secret key of each coin.
    let coinKeys: [Data]
    var delivery: Delivery
    /// Nil until the monitor writes the first state, as for a native message without a state row.
    var state: Chat.LocalMessage.Content.Transfer.State?

    var memo: TransferMemo {
        TransferMemo(entries: coinKeys, totalValue: totalValue)
    }

    /// The monitor still has work for the payment, as the native predicates select messages: a payment of a contact
    /// until its claim ends, and a payment of the user that did not fail until its coins settle.
    var isUnsettled: Bool {
        switch state {
        case let .incoming(state):
            incoming && !state.isTerminal
        case let .outgoing(state):
            !incoming && delivery != .refused && !state.isTerminal
        case .none:
            incoming || delivery != .refused
        }
    }
}

enum ProductChatPaymentStoreError: Error, Equatable {
    case paymentNotFound(Chat.MessageId)
    /// The host has a payment with this id already.
    case duplicate(Chat.MessageId)
    case invalidRow(Chat.MessageId)
}

/// Keeps the payments of the rooms of the chat product. It is also where the transfer monitor writes their states.
protocol ProductChatPaymentStoring: TransferStateStoring {
    /// Keeps a payment of the user. `onSaved` runs inside the same transaction, as it runs inside the transaction of
    /// the native message: the handoff of the coins becomes final exactly when the host keeps the keys and the room.
    /// Throwing from it keeps nothing.
    func saveOutgoing(
        _ payment: ProductChatPayment,
        onSaved: @escaping (any DurableTxRegistrationScope) throws -> Void
    ) async throws

    /// Keeps a payment of a contact once. Gives false when the host kept it before.
    func saveIncoming(_ payment: ProductChatPayment) async throws -> Bool

    /// Every payment, oldest first.
    func payments() async throws -> [ProductChatPayment]

    func setDelivery(_ delivery: ProductChatPayment.Delivery, messageId: Chat.MessageId) async throws
}

/// The payments in the user database (`CDProductChatPayment`), next to the coins whose handoff they carry.
final class ProductChatPaymentStore: ProductChatPaymentStoring, @unchecked Sendable {
    private let databaseService: CoreDataServiceProtocol
    private let dateProvider: any DateProviding

    init(storageFacade: StorageFacadeProtocol, dateProvider: any DateProviding = NowDateProvider()) {
        databaseService = storageFacade.databaseService
        self.dateProvider = dateProvider
    }

    func saveOutgoing(
        _ payment: ProductChatPayment,
        onSaved: @escaping (any DurableTxRegistrationScope) throws -> Void
    ) async throws {
        let keys = try Self.encode(payment.coinKeys)

        try await databaseService.performWrite { context in
            guard try Self.row(payment.messageId, in: context) == nil else {
                throw ProductChatPaymentStoreError.duplicate(payment.messageId)
            }

            try Self.insert(payment, keys: keys, in: context)
            try onSaved(CoreDataRegistrationScope(context: context))
        }
    }

    func saveIncoming(_ payment: ProductChatPayment) async throws -> Bool {
        let keys = try Self.encode(payment.coinKeys)

        return try await databaseService.performWrite { context in
            guard try Self.row(payment.messageId, in: context) == nil else {
                return false
            }

            try Self.insert(payment, keys: keys, in: context)
            return true
        }
    }

    func payments() async throws -> [ProductChatPayment] {
        try await databaseService.performRead { context in
            let entityName = String(describing: CDProductChatPayment.self)
            let request = NSFetchRequest<CDProductChatPayment>(entityName: entityName)
            request.sortDescriptors = [NSSortDescriptor(key: #keyPath(CDProductChatPayment.timestamp), ascending: true)]
            return try context.fetch(request).map(Self.payment)
        }
    }

    func setDelivery(_ delivery: ProductChatPayment.Delivery, messageId: Chat.MessageId) async throws {
        try await databaseService.performWrite { context in
            let row = try Self.existingRow(messageId, in: context)
            guard row.delivery != delivery.rawValue else { return }
            row.delivery = delivery.rawValue
        }
    }

    // MARK: - TransferStateStoring

    func beginIncoming(messageId: Chat.MessageId) async throws -> Date {
        let now = await dateProvider.read()

        return try await databaseService.performWrite { context in
            let row = try Self.existingRow(messageId, in: context)

            if row.status != nil {
                guard let firstAttemptAt = row.firstAttemptAt else {
                    throw CoreDataMapperError.missingRequiredData(
                        keyPath: #keyPath(CDProductChatPayment.firstAttemptAt)
                    )
                }
                return firstAttemptAt
            }

            row.firstAttemptAt = now
            Self.apply(status: IncomingTransferState.Status.detecting.rawValue, actualValue: nil, to: row)
            return now
        }
    }

    func updateIncoming(messageId: Chat.MessageId, state: IncomingTransferState) async throws {
        let now = await dateProvider.read()

        try await databaseService.performWrite { context in
            let row = try Self.existingRow(messageId, in: context)
            if row.firstAttemptAt == nil {
                row.firstAttemptAt = now
            }
            Self.apply(status: state.status.rawValue, actualValue: state.actualValue, to: row)
        }
    }

    func updateOutgoing(messageId: Chat.MessageId, state: OutgoingTransferState) async throws {
        try await databaseService.performWrite { context in
            let row = try Self.existingRow(messageId, in: context)
            Self.apply(status: state.status.rawValue, actualValue: state.actualValue, to: row)
        }
    }
}

private extension ProductChatPaymentStore {
    static func row(_ messageId: Chat.MessageId, in context: NSManagedObjectContext) throws -> CDProductChatPayment? {
        try context.first(for: NSPredicate(format: "%K == %@", #keyPath(CDProductChatPayment.messageId), messageId))
    }

    static func existingRow(
        _ messageId: Chat.MessageId,
        in context: NSManagedObjectContext
    ) throws -> CDProductChatPayment {
        guard let row = try row(messageId, in: context) else {
            throw ProductChatPaymentStoreError.paymentNotFound(messageId)
        }
        return row
    }

    static func insert(_ payment: ProductChatPayment, keys: Data, in context: NSManagedObjectContext) throws {
        let row = try context.insertNew(CDProductChatPayment.self)
        row.messageId = payment.messageId
        row.productId = payment.productId
        row.roomId = payment.roomId
        row.incoming = payment.incoming
        row.timestamp = Int64(clamping: payment.timestamp)
        row.totalValue = String(payment.totalValue)
        row.coinKeys = keys
        row.delivery = payment.delivery.rawValue
    }

    /// Writes nothing when the row holds the state already, so that a repeated state does not wake the rooms.
    static func apply(status: Int16, actualValue: Balance?, to row: CDProductChatPayment) {
        let value = actualValue.map { String($0) }
        guard row.status?.int16Value != status || row.actualValue != value else { return }

        row.status = NSNumber(value: status)
        row.actualValue = value
    }

    static func payment(from row: CDProductChatPayment) throws -> ProductChatPayment {
        let messageId = row.messageId ?? ""
        guard
            let productId = row.productId,
            let roomId = row.roomId,
            let totalValue = row.totalValue.flatMap({ BigUInt($0) }),
            let keys = row.coinKeys,
            let delivery = ProductChatPayment.Delivery(rawValue: row.delivery)
        else {
            throw ProductChatPaymentStoreError.invalidRow(messageId)
        }

        return try ProductChatPayment(
            messageId: messageId,
            productId: productId,
            roomId: roomId,
            incoming: row.incoming,
            timestamp: UInt64(max(row.timestamp, 0)),
            totalValue: totalValue,
            coinKeys: decode(keys),
            delivery: delivery,
            state: state(of: row)
        )
    }

    static func state(of row: CDProductChatPayment) throws -> Chat.LocalMessage.Content.Transfer.State? {
        guard let status = row.status?.int16Value else { return nil }

        let actualValue: Balance?
        if let column = row.actualValue {
            guard let value = BigUInt(column) else {
                throw TransferStateRowError.invalidActualValue(column)
            }
            actualValue = value
        } else {
            actualValue = nil
        }

        if row.incoming {
            guard let status = IncomingTransferState.Status(rawValue: status) else {
                throw TransferStateRowError.unknownStatus(status)
            }
            return .incoming(IncomingTransferState(status: status, actualValue: actualValue))
        }

        guard let status = OutgoingTransferState.Status(rawValue: status) else {
            throw TransferStateRowError.unknownStatus(status)
        }
        return .outgoing(OutgoingTransferState(status: status, actualValue: actualValue))
    }

    static func encode(_ keys: [Data]) throws -> Data {
        try keys.scaleEncoded()
    }

    static func decode(_ data: Data) throws -> [Data] {
        try [Data](scaleDecoder: ScaleDecoder(data: data))
    }
}

private extension [Data] {
    func scaleEncoded() throws -> Data {
        let encoder = ScaleEncoder()
        try encode(scaleEncoder: encoder)
        return encoder.encode()
    }
}
