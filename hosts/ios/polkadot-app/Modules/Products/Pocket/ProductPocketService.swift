import Foundation
import os
import AsyncExtensions
import Products
import TrUAPIHost

/// The Pocket for one session: the collection every surface reads, the handler
/// that draws each product's cards, and the product held behind an opened card.
///
/// Built once where the product resolver and the runtime provider are both in
/// hand, held by ``ServiceCoordinator`` and let go with the session. Everything
/// the Pocket needs comes from here rather than from a process-wide instance of
/// its own, so a sign-out cannot leave one session's Pocket serving the next.
final class ProductPocketService: @unchecked Sendable {
    /// The cards the host holds, which every surface reads and writes.
    let collection: any PocketCardStore
    /// The product held behind the card that was opened last. On the main
    /// actor because what it holds is a web view.
    @MainActor let cardHosts = PocketCardHosts()

    private let logger: CoreEventLoggerProtocol
    private let running = OSAllocatedUnfairLock(initialState: Running())

    /// What this session started, and what a stop gives back. Held under one
    /// lock rather than on an actor because the card views read it from
    /// whatever executor they resumed on, and hopping to answer would mean
    /// asserting an isolation they do not have.
    private struct Running {
        var drawing: Drawing?
        var handlers: PocketHandlers?
        var following: Task<Void, Never>?
        var installations = 0
    }

    /// What the cards are drawn through, once a session has started the Pocket.
    private struct Drawing {
        let faces: any PocketFaceSourcing
        let products: any ProductResolving
        let productImages: ProductImageSources
    }

    init(collection: any PocketCardStore, logger: CoreEventLoggerProtocol = Logger.shared) {
        self.collection = collection
        self.logger = logger
    }

    static func make(
        runtimeProvider: any TrUAPIHostRuntimeProviding,
        logger: CoreEventLoggerProtocol = Logger.shared
    ) -> ProductPocketService {
        ProductPocketService(
            collection: CorePocketCardStore(runtimeProvider: runtimeProvider, pinned: AssetPinnedPocketCards()),
            logger: logger
        )
    }

    /// How many times this process has started a Pocket. A sign-out and back in
    /// starts another, and everything read from the one before it is finished:
    /// cards key their drawing on this so they start again on the new one
    /// rather than hold a stream that has ended.
    var installation: Int { running.withLock { $0.installations } }

    /// The collection as the Wallet tab draws it, followed from storage so it
    /// shows what the host holds without waiting on any product's worker, and
    /// shows every change whoever made it.
    func cards() -> AnyAsyncSequence<[PocketCardViewModel]> {
        let provider = PocketCardsProvider(store: collection)

        return collection
            .observeCards()
            .map { await provider.cards($0) }
            .eraseToAnyAsyncSequence()
    }

    /// The faces `key` is drawn with: the one the host already holds, then
    /// every one its product draws.
    func faces(for key: PocketCardKey) -> AsyncStream<RendererNode> {
        guard let faces = running.withLock({ $0.drawing?.faces }) else {
            logger.warning("[pocket] no face source yet; \(key.cardId.value) draws what is kept")
            return AsyncStream { $0.finish() }
        }

        return faces.faces(for: key)
    }

    func send(action: String, payload: Data, for key: PocketCardKey) {
        running.withLock { $0.drawing?.faces }?.send(action: action, payload: payload, for: key)
    }

    /// Resolves the images inside `productId`'s faces, out of that product's
    /// own worker archive. Nil before the session has started the Pocket.
    func images(of productId: ProductId) -> ProductImageResolver? {
        guard let drawing = running.withLock({ $0.drawing }) else { return nil }

        let name = PocketWorkerArchiveName(
            productId: productId,
            published: { try? await drawing.products.resolve(productId).contentId(for: .worker) }
        )

        return drawing.productImages.resolver(contentId: { await name.resolve() })
    }

    /// Follow the session's collection and draw its cards through the shared workers.
    func start(runtime: @escaping ProductWorkerRuntimeProvider, flowState: SPAFlowState) {
        let handlers = PocketHandlers(
            runtime: runtime,
            published: PublishedPocketCards.makeDefault(products: flowState.productResolver),
            logger: logger
        )

        let drawing = Drawing(
            faces: RealPocketFaceSource(
                store: { [collection] in collection },
                streams: handlers,
                logger: logger
            ),
            products: flowState.productResolver,
            productImages: flowState.productImages
        )

        install(drawing: drawing, handlers: handlers)
        follow()
    }

    /// Release the claims held by the cards from this session.
    func stop() {
        install(drawing: nil, handlers: nil)
    }
}

private extension ProductPocketService {
    var handlers: PocketHandlers? { running.withLock { $0.handlers } }

    private func install(drawing: Drawing?, handlers: PocketHandlers?) {
        let previous = running.withLock { running in
            let previous = running.handlers
            running.drawing = drawing
            running.handlers = handlers
            running.following?.cancel()
            running.following = nil
            if drawing != nil { running.installations += 1 }
            return previous
        }
        previous?.stop()
    }

    func follow() {
        let task = Task { [weak self, collection, logger] in
            do {
                for try await cards in collection.observeCards() {
                    guard let self else { return }

                    handlers?.settle(cards)
                }
            } catch {
                logger.error("[pocket] the Pocket stopped following the collection: \(error)")
            }
        }

        // A stop can land while `start` is still wiring. Installing the follow
        // task after that would leave one starting handlers for a session that
        // has already ended.
        let stopped = running.withLock { running -> Bool in
            guard running.drawing != nil else { return true }

            running.following = task
            return false
        }
        if stopped { task.cancel() }
    }
}

extension ProductPocketService {
    /// The session's Pocket, for the two places that cannot be handed one: the
    /// deeplink services the tab bar assembles before the session's own wiring
    /// is reachable, and the wireframe that opens a card from it.
    static var current: ProductPocketService? {
        RootDependencyLocator.getDependency()
    }
}
