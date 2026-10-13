import Foundation
import os
import AsyncExtensions
import BulletinChain
import ChainRegistry
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

    private let logger: LoggerProtocol
    private let running = OSAllocatedUnfairLock(initialState: Running())

    /// What this session started, and what a stop gives back. Held under one
    /// lock rather than on an actor because the card views read it from
    /// whatever executor they resumed on, and hopping to answer would mean
    /// asserting an isolation they do not have.
    private struct Running {
        var workers: (any TrUAPIWorkerManaging)?
        var drawing: Drawing?
        var handlers: PocketHandlers?
        var following: Task<Void, Never>?
        var installations = 0
    }

    /// What the cards are drawn through, once a session has started the Pocket.
    private struct Drawing {
        let faces: any PocketFaceSourcing
        let products: any ProductResolving
        let dotNsResolver: any DotNsResolverProtocol
        let ipfsUrl: @Sendable (String) -> URL?
    }

    init(collection: any PocketCardStore, logger: LoggerProtocol = Logger.shared) {
        self.collection = collection
        self.logger = logger
    }

    /// The reserved product a host-placed card sits on is named per network, so
    /// the collection needs the network's dotNS suffix. It is resolved before
    /// anything the Pocket is reachable from exists, which is why this is not
    /// waited on here.
    static func make(
        tld: any DotNsTldProviding = DotNsTldProviderFacade.shared,
        logger: LoggerProtocol = Logger.shared
    ) -> ProductPocketService? {
        guard let suffix = try? tld.currentTldOrError() else {
            logger.error("[pocket] no network suffix yet, so there is no Pocket to serve")
            return nil
        }

        return ProductPocketService(
            collection: CoreDataPocketCardStore(pinned: AssetPinnedPocketCards(tld: suffix)),
            logger: logger
        )
    }

    /// What runs every product's worker. Nil until the session has started the
    /// Pocket, which is when a chat bot falls back to the native runtime.
    var workers: (any TrUAPIWorkerManaging)? { running.withLock { $0.workers } }

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
    func images(of productId: ProductId) -> PocketImageResolver? {
        guard let drawing = running.withLock({ $0.drawing }) else { return nil }

        let name = PocketWorkerArchiveName(
            productId: productId,
            published: { try? await drawing.products.resolve(productId).contentId(for: .worker) }
        )

        return PocketImageResolver(
            contentId: { await name.resolve() },
            archive: ProductWorkerArchive(dotNsResolver: drawing.dotNsResolver),
            ipfsUrl: drawing.ipfsUrl
        )
    }

    /// Starts the worker manager the core runs on, then follows the collection:
    /// a product the user holds cards for gets a handler, and loses it when the
    /// last of its cards goes.
    func start(
        runtimeProvider: any TrUAPIHostRuntimeProviding,
        flowState: SPAFlowState,
        productFileProvider: any ChatProductFileProviding,
        chainRegistry: ChainRegistryProtocol,
        gameReminders: any ProductGameReminderScheduling
    ) {
        let manager = TrUAPIWorkerManager(
            builder: TrUAPIWorkerBuilder(
                environment: { [weak runtimeProvider, logger] in
                    guard let runtimeProvider else { throw PocketServiceError.gone }

                    return try RustRuntimeEnvironment(
                        runtime: runtimeProvider.sharedRuntime(),
                        chainRegistry: chainRegistry,
                        notificationScheduler: ProductNotificationScheduler.shared,
                        gameReminders: gameReminders,
                        ipfsFetcher: IpfsFetcher(ipfsBaseURL: AppConfig.KnownIPFS.main),
                        hostProvider: flowState.hostProvider,
                        logger: logger
                    )
                },
                products: flowState.productResolver,
                dotNsResolver: flowState.dotNsResolver,
                productFileProvider: productFileProvider,
                logger: logger
            ),
            collection: collection,
            references: { [weak runtimeProvider] in
                guard let runtimeProvider else { throw PocketServiceError.gone }

                return try runtimeProvider.sharedRuntime()
            },
            logger: logger
        )

        runtimeProvider.attach(workerManager: manager)

        let handlers = PocketHandlers(
            workers: manager,
            published: PublishedPocketCards.makeDefault(products: flowState.productResolver),
            logger: logger
        )

        let converter = HexToCIDConverter(ipfsBaseURL: AppConfig.KnownIPFS.main)
        let drawing = Drawing(
            faces: RealPocketFaceSource(
                store: { [collection] in collection },
                streams: handlers,
                logger: logger
            ),
            products: flowState.productResolver,
            dotNsResolver: flowState.dotNsResolver,
            ipfsUrl: { converter.ipfsURL(cid: $0) }
        )

        install(manager: manager, drawing: drawing, handlers: handlers)
        follow()
    }

    /// Stops the handlers and the workers behind them, and gives up the product
    /// behind an opened card. All of it belongs to the session's runtime
    /// provider, so none of it may outlive it.
    func stop() {
        install(manager: nil, drawing: nil, handlers: nil)
        Task { @MainActor in cardHosts.release() }
    }
}

private extension ProductPocketService {
    var handlers: PocketHandlers? { running.withLock { $0.handlers } }

    /// Nothing else holds a way back to the handlers and workers a replaced
    /// manager is still running, so they are torn down here.
    private func install(
        manager: (any TrUAPIWorkerManaging)?,
        drawing: Drawing?,
        handlers: PocketHandlers?
    ) {
        let previous = running.withLock { running -> (any TrUAPIWorkerManaging, PocketHandlers?)? in
            let previousWorkers = running.workers
            let previousHandlers = running.handlers

            running.workers = manager
            running.drawing = drawing
            running.handlers = handlers
            running.following?.cancel()
            running.following = nil
            if manager != nil { running.installations += 1 }

            guard let previousWorkers else {
                previousHandlers?.stop()
                return nil
            }
            return (previousWorkers, previousHandlers)
        }

        guard let previous else { return }

        previous.1?.stop()
        Task { await previous.0.shutdown() }
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

enum PocketServiceError: Error {
    case gone
}

extension ProductPocketService {
    /// The session's Pocket, for the two places that cannot be handed one: the
    /// deeplink services the tab bar assembles before the session's own wiring
    /// is reachable, and the wireframe that opens a card from it.
    static var current: ProductPocketService? {
        RootDependencyLocator.getDependency()
    }
}
