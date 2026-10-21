import Foundation
import AsyncExtensions
import Products
import StructuredConcurrency

/// The wake at the start of a product that has no room yet (W2 of 07-LATER §10.2, 06-OPEN-ITEMS O25,
/// 02-DESIGN §22 item 4).
///
/// No room screen can start a product that has no room, so the product cannot make its rooms without this wake.
/// The core holds the worker for 30 seconds through `wake_product`, and then releases it. A product that has a room,
/// or whose rooms are not known, gets no wake.
struct ProductFirstRoomHold: Sendable {
    /// The rooms of the product, as the chat surface of the product gives them.
    typealias RoomsOfProduct = @Sendable (ProductWorkerContext) async throws -> AnyAsyncSequence<[RoomInfo]>

    let productId: ProductId
    let workers: any TrUAPIWorkerManaging
    let rooms: @Sendable () async throws -> AnyAsyncSequence<[RoomInfo]>
    let logger: LoggerProtocol

    /// Wakes the product when it has no room. A cancel before the wake stops it.
    func run() async {
        do {
            var current = try await rooms().makeAsyncIterator()
            guard let first = try await current.next(), first.isEmpty else { return }
        } catch {
            logger.debug("[truapi] \(productId) gets no start wake, because its rooms are not known: \(error)")
            return
        }

        guard !Task.isCancelled else { return }

        logger.debug("[truapi] \(productId) has no room, so it gets a start wake")
        await workers.wake(productId, reason: .productStart)
    }
}
