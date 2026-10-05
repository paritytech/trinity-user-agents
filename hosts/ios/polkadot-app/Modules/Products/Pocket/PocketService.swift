import Foundation
import os
import BulletinChain
import ChainRegistry
import Products
import TrUAPIHost

/// The Pocket for one session: the collection every surface reads, the workers
/// that draw its cards, and the product held behind an opened card.
///
/// Built once where the product resolver and the runtime provider are both in
/// hand, held by ``ServiceCoordinator`` and let go with the session. Everything
/// the Pocket needs comes from here rather than from a process-wide instance of
/// its own, so a sign-out cannot leave one session's Pocket serving the next.
@MainActor
final class PocketService {
    /// The cards the host holds, which every surface reads and writes.
    let collection: any PocketCardStore
    /// The product held behind the card that was opened last.
    let cardHosts = PocketCardHosts()

    private let logger: LoggerProtocol
    private var drawing: Drawing?
    private var installations = 0

    /// Read without isolation, because bot discovery asks for it from whatever
    /// executor it resumed on. Hopping to the main actor to answer would mean
    /// asserting an isolation that discovery does not have.
    private let managerHeld = OSAllocatedUnfairLock<(any TrUAPIWorkerManaging)?>(initialState: nil)

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
    ) -> PocketService? {
        guard let suffix = try? tld.currentTldOrError() else {
            logger.error("[pocket] no network suffix yet, so there is no Pocket to serve")
            return nil
        }

        return PocketService(
            collection: CoreDataPocketCardStore(pinned: AssetPinnedPocketCards(tld: suffix)),
            logger: logger
        )
    }

    /// Where a card's faces come from. Nil until the session has started the
    /// Pocket, which is also while there is no runtime for a worker to open
    /// against.
    var faces: (any PocketFaceSourcing)? { drawing?.faces }

    /// What runs every product's worker. Nil until the session has started the
    /// Pocket, which is when a chat bot falls back to the native runtime.
    nonisolated var manager: (any TrUAPIWorkerManaging)? { managerHeld.withLock { $0 } }

    /// How many times this process has started a Pocket. A sign-out and back in
    /// starts another, and everything read from the one before it is finished:
    /// cards key their drawing on this so they start again on the new one
    /// rather than hold a stream that has ended.
    var installation: Int { installations }

    /// Resolves the images inside `productId`'s faces, out of that product's
    /// own worker archive. Nil before the session has started the Pocket.
    func images(of productId: ProductId) -> PocketImageResolver? {
        guard let drawing else { return nil }

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

    /// Starts the workers and the face source the cards read, and wires the
    /// manager into the runtime the core runs on.
    func start(
        runtimeProvider: any TrUAPIHostRuntimeProviding,
        flowState: SPAFlowState,
        productFileProvider: any ChatProductFileProviding,
        chainRegistry: ChainRegistryProtocol
    ) {
        let manager = TrUAPIWorkerManager(
            builder: TrUAPIWorkerBuilder(
                environment: { [weak runtimeProvider, logger] in
                    guard let runtimeProvider else { throw PocketServiceError.gone }

                    return try RustRuntimeEnvironment(
                        runtime: runtimeProvider.sharedRuntime(),
                        chainRegistry: chainRegistry,
                        notificationScheduler: ProductNotificationScheduler.shared,
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
        replace(with: manager)

        let converter = HexToCIDConverter(ipfsBaseURL: AppConfig.KnownIPFS.main)
        drawing = Drawing(
            faces: RealPocketFaceSource(
                store: { [collection] in collection },
                streams: TrUAPIPocketFaceStreams(
                    workers: manager,
                    publishedCards: PublishedPocketCards.makeDefault(products: flowState.productResolver),
                    logger: logger
                ),
                logger: logger
            ),
            products: flowState.productResolver,
            dotNsResolver: flowState.dotNsResolver,
            ipfsUrl: { converter.ipfsURL(cid: $0) }
        )
        installations += 1
    }

    /// Stops the workers and gives up the product behind an opened card. Both
    /// belong to the session's runtime provider, so neither may outlive it.
    func stop() {
        replace(with: nil)
        drawing = nil
        cardHosts.release()
    }

    /// Nothing else holds a way back to the workers a replaced manager is
    /// still running, so it is shut down here.
    private func replace(with manager: (any TrUAPIWorkerManaging)?) {
        let previous = managerHeld.withLock { held -> (any TrUAPIWorkerManaging)? in
            let previous = held
            held = manager
            return previous
        }

        guard let previous else { return }

        Task { await previous.shutdown() }
    }
}

enum PocketServiceError: Error {
    case gone
}

extension PocketService {
    /// The session's Pocket, for the two places that cannot be handed one: the
    /// card views SwiftUI builds, and the URL services the tab bar assembles
    /// before the session's own wiring is reachable.
    static var current: PocketService? {
        RootDependencyLocator.getDependency()
    }
}
