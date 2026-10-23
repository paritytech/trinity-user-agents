import AsyncExtensions
import BigInt
import CoreData
import Foundation
import FoundationExt
import SubstrateSdk

@testable import polkadot_app

/// The payments of a room of the chat product on an in-memory user store, so monitor tests exercise the real payment
/// store and the state events the rooms read.
final class TransferStateTestWorld {
    enum Direction {
        case incoming
        case outgoing
    }

    let facade = UserDataStorageTestFacade()
    let store: ProductChatPaymentStore
    let payments: ProductChatPayments

    init(dateProvider: any DateProviding = NowDateProvider()) {
        store = ProductChatPaymentStore(storageFacade: facade, dateProvider: dateProvider)
        payments = ProductChatPayments(
            store: store,
            delivery: ProductQueryingDelivery(querying: ScriptedProductChatQuerying()),
            worker: NoProductHopUploads(),
            logger: StubLogger()
        )
    }

    /// A payment the product has, with no state yet, as a payment of a contact waits for its first claim.
    @discardableResult
    func saveTransfer(
        _ direction: Direction,
        totalValue: Balance = 10,
        coinKeys: [Data] = [Data(repeating: 0x07, count: 32)]
    ) async throws -> ProductChatPayment {
        let payment = ProductChatPayment(
            messageId: UUID().uuidString,
            productId: "chat.dot",
            roomId: "dm-01",
            incoming: direction == .incoming,
            timestamp: 1,
            totalValue: totalValue,
            coinKeys: coinKeys,
            delivery: .delivered,
            state: nil
        )
        try await payments.adopt([AdoptedChatPayment(payment: payment, firstAttemptAt: nil)])
        return payment
    }

    func transfer(_ messageId: Chat.MessageId) async throws -> ProductChatPayment? {
        try await store.payments().first { $0.messageId == messageId }
    }

    /// The state of `messageId` now and after each change the rooms see, a repeated state once.
    func stateStream(of messageId: Chat.MessageId) async -> AsyncStream<Chat.LocalMessage.Content.Transfer.State?> {
        let events = await payments.events()
        let current = try? await transfer(messageId)?.state

        return AsyncStream { continuation in
            let task = Task {
                var last = current
                continuation.yield(current)
                for await event in events {
                    guard case let .state(id, state) = event, id == messageId, state != last else { continue }
                    last = state
                    continuation.yield(state)
                }
                continuation.finish()
            }
            continuation.onTermination = { _ in task.cancel() }
        }
    }
}

extension AsyncStream where Element: Equatable {
    /// Elements up to and including the first one satisfying `predicate`.
    func collect(until predicate: @escaping (Element) -> Bool) async -> [Element] {
        var collected: [Element] = []
        for await element in self {
            collected.append(element)
            if predicate(element) { break }
        }
        return collected
    }
}
