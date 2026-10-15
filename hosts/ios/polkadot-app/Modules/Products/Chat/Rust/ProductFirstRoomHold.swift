import Foundation
import AsyncExtensions
import Products
import StructuredConcurrency

/// The short hold of the worker at the start of a product that has no room yet (06-OPEN-ITEMS O25,
/// 02-DESIGN §22 item 4).
///
/// No room screen can start a product that has no room, so the product cannot make its rooms without this hold.
/// The hold asks for the worker, waits until the first room of the product appears, waits `settle` more so the
/// product can make its other rooms, and gives the request back. It gives the request back at `limit` when no
/// room appears. A product that has a room, or whose rooms are not known, gets no hold.
struct ProductFirstRoomHold: Sendable {
    struct Limits: Sendable {
        /// The longest time that the hold lasts.
        var limit: Duration = .seconds(30)
        /// The time that the hold continues after the first room of the product appears.
        var settle: Duration = .seconds(2)
    }

    /// The rooms of the product, as the chat surface of the product gives them.
    typealias RoomsOfProduct = @Sendable (ProductWorkerContext) async throws -> AnyAsyncSequence<[RoomInfo]>

    let productId: ProductId
    let workers: any TrUAPIWorkerManaging
    let limits: Limits
    let rooms: @Sendable () async throws -> AnyAsyncSequence<[RoomInfo]>
    let logger: LoggerProtocol

    /// Holds the worker when the product has no room. A cancel ends the hold and gives the request back.
    func run() async {
        do {
            var current = try await rooms().makeAsyncIterator()
            guard let first = try await current.next(), first.isEmpty else { return }
        } catch {
            logger.debug("[truapi] \(productId) gets no first-room hold, because its rooms are not known: \(error)")
            return
        }

        guard !Task.isCancelled else { return }

        do {
            _ = try await workers.ensureWorker(for: productId)
        } catch {
            workers.releaseWorker(for: productId)
            logger.warning("[truapi] \(productId)'s worker did not start for its first room: \(error)")
            return
        }

        logger.debug("[truapi] \(productId) holds its worker until it makes its first room")
        do {
            try await withTimeout(limits.limit) { [rooms, limits] in
                for try await list in try await rooms() where !list.isEmpty {
                    try await Task.sleep(for: limits.settle)
                    return
                }
            }
        } catch {
            logger.debug("[truapi] \(productId)'s first-room hold ended without a room: \(error)")
        }

        workers.releaseWorker(for: productId)
        logger.debug("[truapi] \(productId) gave back its first-room hold")
    }
}
