import AsyncExtensions
import Foundation
import FoundationExt
import Keystore_iOS
import Operation_iOS
import Products
import StructuredConcurrency

/// What the provider reports: the products it serves bots for, as soon as they are known, and
/// each bot on its own once its manifest resolved and its worker archive downloaded. A product
/// whose manifest cannot be read never gets a `botBuilt`.
enum ProductBotEvent {
    case listed([Product])
    case botBuilt(ProductBot)
}

protocol ProductBotProviding {
    func observeBots() -> AnyAsyncSequence<ProductBotEvent>
}

final class ProductBotProvider: ProductBotProviding {
    private let productProvider: StreamableProvider<Product>
    private let botFactory: ProductBotFactory
    private let dotNsResolver: DotNsResolverProtocol
    private let productResolver: ProductResolving
    private let tldProvider: DotNsTldProviding
    private let settingsManager: SettingsManagerProtocol
    private let logger: LoggerProtocol

    /// Roughly a minute of holding up every other product's bot.
    private static let tldAttempts = 20

    init(
        productProvider: StreamableProvider<Product>,
        botFactory: ProductBotFactory,
        dotNsResolver: DotNsResolverProtocol,
        productResolver: ProductResolving,
        tldProvider: DotNsTldProviding = DotNsTldProviderFacade.shared,
        settingsManager: SettingsManagerProtocol = SettingsManager.shared,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.productProvider = productProvider
        self.botFactory = botFactory
        self.dotNsResolver = dotNsResolver
        self.productResolver = productResolver
        self.tldProvider = tldProvider
        self.settingsManager = settingsManager
        self.logger = logger
    }

    /// Reports every product change at once and never waits on a build: each product without a
    /// bot gets its own build task, which reports the bot when it is ready. A product that leaves
    /// the list has its build cancelled; one whose build failed is tried again on the next change.
    func observeBots() -> AnyAsyncSequence<ProductBotEvent> {
        let (stream, continuation) = AsyncThrowingStream<ProductBotEvent, Error>.makeStream()
        let builds = ProductBotBuilds()

        let observation = Task { [self] in
            let productDicts = productProvider.asyncStream()
                .scan([String: Product]()) { dict, changes in
                    changes.mergeToDict(dict)
                }

            do {
                for try await productDict in productDicts {
                    let products = await listProducts(from: productDict)
                    continuation.yield(.listed(products))

                    await builds.keep(only: Set(products.map(\.identifier)))

                    for product in products {
                        guard await builds.needsBuild(product.identifier) else { continue }

                        await builds.register(product.identifier) { [self] in
                            let bot = await makeBot(for: product)
                            if let bot { continuation.yield(.botBuilt(bot)) }
                            await builds.finished(product.identifier, built: bot != nil)
                        }
                    }
                }

                continuation.finish()
            } catch {
                continuation.finish(throwing: error)
            }

            await builds.cancelAll()
        }

        continuation.onTermination = { _ in observation.cancel() }

        return stream.eraseToAnyAsyncSequence()
    }

    #if targetEnvironment(simulator)
        /// The product the truapi E2E launcher wants a chat bot for, named by environment
        /// rather than installed through the UI so `make ios-chat-run` needs no taps.
        /// It still goes through the normal resolve path, so a manifest product is served
        /// from its own worker subname exactly as it would be in a real install.
        private static func simulatorChatProduct() -> Product? {
            let environment = ProcessInfo.processInfo.environment
            guard let productId = environment["TRUAPI_IOS_E2E_CHAT_PRODUCT_HOST"]?
                .trimmingCharacters(in: .whitespacesAndNewlines),
                !productId.isEmpty
            else {
                return nil
            }

            let name = environment["TRUAPI_IOS_E2E_CHAT_PRODUCT_NAME"]?
                .trimmingCharacters(in: .whitespacesAndNewlines)
            let displayName = name?.nilIfEmpty ?? productId

            return Product(id: productId, name: displayName)
        }
    #endif
}

private extension ProductBotProvider {
    func listProducts(from productDict: [String: Product]) async -> [Product] {
        var products = Array(productDict.values)

        #if targetEnvironment(simulator)
            if let injected = Self.simulatorChatProduct(),
               !products.contains(where: { $0.identifier == injected.identifier }) {
                products.append(injected)
            }
        #endif

        // After the E2E injection, so a launcher naming a host-placed product keeps the
        // display name it asked for.
        for hostPlaced in await hostPlacedProducts() {
            guard !products.contains(where: { $0.identifier == hostPlaced.identifier }) else {
                continue
            }

            products.append(hostPlaced)
        }

        return products
    }

    /// Unioned into the stream rather than written to the product repository: nothing installs
    /// them, so nothing can uninstall them either. Pocket refuses to remove a privileged card;
    /// this gets the same result without a second copy of the collection to keep in step.
    ///
    /// The TLD wait is bounded because every other product's bot is built after it, and a device
    /// that can never read the TLD must not cost the products that do not need it.
    func hostPlacedProducts() async -> [Product] {
        guard settingsManager.isHostPlacementEnabled else { return [] }

        guard let tld = await tldProvider.tldRetrying(attempts: Self.tldAttempts) else { return [] }

        return HostPlacedProducts.all.map {
            Product(id: $0.productId(tld), name: $0.fallbackName)
        }
    }

    /// The last async seam before a bot is built: the file provider that serves the worker runs
    /// synchronously, so the worker's archive and entry module have to be known by now.
    func makeBot(for product: Product) async -> ProductBot? {
        guard let resolved = await resolveForBot(product), !Task.isCancelled else { return nil }

        return botFactory.create(resolved: resolved)
    }

    /// Nil for a product whose published manifest is broken. That is the product's bug, and
    /// running the base name's archive in its place would serve something it never declared.
    ///
    /// A name that cannot be read still degrades to the legacy shape. Since workers now come only
    /// from a manifest, that yields a bot for hand-installed debug scripts and nothing else — a
    /// published product's bot needs its worker subname, so it needs a resolve that succeeded.
    func resolveForBot(_ product: Product) async -> ResolvedProduct? {
        do {
            let resolved = try await productResolver.resolve(product.identifier)
            await warmWorkerArchive(of: resolved)

            return resolved
        } catch ProductResolutionError.malformedManifest {
            logger.error("Skipping the chat bot for \(product.identifier): its manifest is malformed")
            return nil
        } catch {
            logger.error("Falling back to the legacy chat worker for \(product.identifier): \(error)")
            return .legacy(id: product.identifier, displayName: product.name)
        }
    }

    /// A manifest worker ships its own archive, which nothing else downloads. Legacy workers share
    /// the base name's archive, warmed when the product itself was opened.
    ///
    /// A failed warm only fails this bot, at the point the engine asks for the files. It must not
    /// turn a manifest product into a legacy one and serve the base archive as its worker.
    func warmWorkerArchive(of resolved: ResolvedProduct) async {
        // Chat is the only surface iOS runs a worker on, so anything else is a download nothing reads.
        guard let worker = resolved.executables.worker, worker.serves(.chat) else { return }

        do {
            _ = try await dotNsResolver.resolveToLocalURL(dotNsName: worker.identifier)
        } catch {
            logger.error("Failed to warm the chat worker archive \(worker.identifier): \(error)")
        }
    }
}

/// The build task of every product that has no bot yet, keyed by product id. A product counts as
/// having a bot from the moment its build starts, so a second list change never builds it twice.
private actor ProductBotBuilds {
    private var tasks: [String: Task<Void, Never>] = [:]
    private var built: Set<String> = []

    func needsBuild(_ productId: String) -> Bool {
        tasks[productId] == nil && !built.contains(productId)
    }

    func register(_ productId: String, _ build: @escaping @Sendable () async -> Void) {
        tasks[productId] = Task(operation: build)
    }

    /// A build cancelled by `keep(only:)` still reaches here; it must not record a bot for a
    /// product that left, nor forget a build registered since for the same product.
    func finished(_ productId: String, built isBuilt: Bool) {
        guard !Task.isCancelled else { return }

        tasks[productId] = nil

        if isBuilt {
            built.insert(productId)
        }
    }

    /// Cancels the builds and forgets the bots of the products not in `productIds`.
    func keep(only productIds: Set<String>) {
        for (productId, task) in tasks where !productIds.contains(productId) {
            task.cancel()
            tasks[productId] = nil
        }

        built = built.intersection(productIds)
    }

    func cancelAll() {
        tasks.values.forEach { $0.cancel() }
        tasks = [:]
    }
}
