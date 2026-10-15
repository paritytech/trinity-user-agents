import Foundation
import Operation_iOS
import Products
import AsyncExtensions

@MainActor
final class SPANativeRuntimeInteractor {
    weak var presenter: SPAInteractorOutputProtocol?

    private let nativeApi: ProductsNativeApiProtocol
    private let scriptsFactory: SPAScriptsMaking
    private let dotNsResolver: DotNsResolverProtocol
    private let productResolver: ProductResolving
    private let schemeHandlerProxy: SchemeHandlerProxy
    private let configuration: SPAConfiguration
    private let logger: LoggerProtocol
    private let productRepository: AnyDataProviderRepository<Product>
    private let chatProviderFactory: ChatContactDataProviderMaking
    /// Keeps the product's worker alive while the screen is open: acquired in
    /// `setup`, released on deinit.
    private let workerManager: ProductWorkerManaging
    private let workerProductId: ProductId
    private var workerToken: ProductWorkerToken?
    private var workerTask: Task<Void, Never>?

    private var containerBridge: ContainerBridge?
    private var jsEngine: JSEngineProtocol?
    private var engineMonitor: JSEngineMonitor?
    // Held so the synchronous `hasChatEntry` can answer from the manifest without re-resolving.
    private var resolvedProduct: ResolvedProduct?

    private var setupTask: Task<Void, Never>?
    private var openChatTask: Task<Void, Never>?
    private var progressTask: Task<Void, Never>?

    init(
        nativeApi: ProductsNativeApiProtocol,
        scriptsFactory: SPAScriptsMaking,
        dotNsResolver: DotNsResolverProtocol,
        productResolver: ProductResolving,
        schemeHandlerProxy: SchemeHandlerProxy,
        configuration: SPAConfiguration,
        logger: LoggerProtocol,
        productRepository: AnyDataProviderRepository<Product>,
        chatProviderFactory: ChatContactDataProviderMaking,
        workerManager: ProductWorkerManaging,
        workerProductId: ProductId
    ) {
        self.nativeApi = nativeApi
        self.scriptsFactory = scriptsFactory
        self.dotNsResolver = dotNsResolver
        self.productResolver = productResolver
        self.schemeHandlerProxy = schemeHandlerProxy
        self.configuration = configuration
        self.logger = logger
        self.productRepository = productRepository
        self.chatProviderFactory = chatProviderFactory
        self.workerManager = workerManager
        self.workerProductId = workerProductId
    }

    deinit {
        setupTask?.cancel()
        openChatTask?.cancel()
        progressTask?.cancel()
        workerTask?.cancel()
        workerToken?.unlock()

        engineMonitor?.stop()

        Task { [containerBridge, jsEngine] in
            await containerBridge?.dispose()
            await jsEngine?.destroy()
        }
    }
}

extension SPANativeRuntimeInteractor: SPAInteractorInputProtocol {
    func setup(engine: JSEngineProtocol) {
        jsEngine = engine
        acquireWorker()
        setupTask = Task { [weak self] in
            await self?.performSetup(engine: engine)
        }
    }

    func retry() {
        guard let engine = jsEngine else { return }

        setupTask?.cancel()
        setupTask = Task { [weak self] in
            await self?.performSetup(engine: engine)
        }
    }

    func hasChatEntry() -> Bool {
        // Declared, or not offered: a product with no worker manifest has no chat surface here.
        resolvedProduct?.executables.worker?.serves(.chat) ?? false
    }

    func openChat() {
        guard openChatTask == nil else {
            return
        }

        openChatTask = Task { [weak self] in
            do {
                guard let extensionId = try await self?.enableProduct() else {
                    return
                }

                if let chatId = try await self?.waitProductChat(for: extensionId) {
                    self?.presenter?.didPrepareChat(chatId: chatId)
                }
            } catch {
                self?.logger.error("SPA: Failed to open chat: \(error)")
            }

            guard !Task.isCancelled else {
                return
            }

            await MainActor.run { [weak self] in
                self?.openChatTask = nil
            }
        }
    }
}

private extension SPANativeRuntimeInteractor {
    /// Holds the worker for the screen's lifetime. Acquire waits for the boot, so
    /// if the screen is torn down first the token is released instead of leaked.
    func acquireWorker() {
        workerTask = Task { [weak self, workerManager, workerProductId] in
            let lease = await workerManager.acquire(productId: workerProductId)

            guard let self, !Task.isCancelled else {
                lease.token.unlock()
                return
            }

            workerToken = lease.token
        }
    }

    func enableProduct() async throws -> ChatExtension.Id {
        let product = Product(
            id: configuration.page.host.toDotDomain(),
            name: resolvedProduct?.displayName ?? configuration.page.host.name
        )
        let extensionId = product.extensionId

        let existingProduct = try await productRepository
            .fetchOperation(by: { product.identifier }, options: RepositoryFetchOptions())
            .asyncExecute()

        if existingProduct == nil {
            try await productRepository.saveOperation({ [product] }, { [] }).asyncExecute()
        }

        return extensionId
    }

    func waitProductChat(for extensionId: ChatExtension.Id) async throws -> Chat.Id? {
        let stream = chatProviderFactory.subscribeChatsWithPredicate(.roomChatsForExtension(extensionId))

        for try await chats in stream {
            if let chat = chats.first {
                return chat.chatId
            }
        }

        return nil
    }

    func subscribeProgress(domain: String) {
        progressTask?.cancel()
        progressTask = Task { [weak self] in
            guard let stream = self?.dotNsResolver.progressStream(dotNsName: domain) else { return }
            do {
                for try await progress in stream {
                    guard !Task.isCancelled else { return }
                    self?.presenter?.didUpdateLoadProgress(progress)
                }
            } catch {
                self?.logger.error("Resolve progress error: \(error)")
            }
        }
    }

    /// Cancellation guards after every await: teardown cancels `setupTask`
    /// synchronously on the main actor, so a resumed setup must not assign
    /// resources (leaked monitor/bridge) or deliver stale results.
    func performSetup(engine: JSEngineProtocol) async {
        do {
            let domain = configuration.page.host.toDotDomain()

            let resolved = try await productResolver.resolve(domain)
            resolvedProduct = resolved

            // Bytes come from the app executable's subname; the origin stays the base domain, which
            // is what permission grants and web storage are keyed by.
            let contentId = resolved.contentId(for: configuration.executable)

            subscribeProgress(domain: contentId)

            let contentURL = try await dotNsResolver.resolveToLocalURL(dotNsName: contentId)
            let schemeHandler = makeSchemeHandler(
                domain: domain,
                contentURL: contentURL
            )

            guard let productURL = schemeHandler.getProductUrl() else { return }

            logger.debug("SPA: '\(domain)' content resolved to \(contentURL.path)")

            schemeHandlerProxy.setHandler(schemeHandler)

            let scripts = try scriptsFactory.makeScripts()
            try await engine.initialize(with: scripts)

            guard !Task.isCancelled else { return }

            let monitor = JSEngineMonitor(engine: engine)
            monitor.start()
            engineMonitor = monitor

            logger.debug("SPA: Engine initialized")

            let bridge = ContainerBridge(engine: engine, logger: logger)
            await bridge.registerHostApiHandlers(
                nativeApi: nativeApi,
                onRenderWidget: { _, _ in }
            )
            await bridge.install()

            guard !Task.isCancelled else {
                await bridge.dispose()
                return
            }

            containerBridge = bridge
            logger.debug("SPA: ContainerBridge installed")

            let initialURL = configuration.page.applied(to: productURL)
            presenter?.didRequestNavigation(to: initialURL)
        } catch {
            guard !Task.isCancelled else { return }
            logger.error("SPA: Setup failed: \(error)")
            presenter?.didFail(error: error)
        }
    }

    func makeSchemeHandler(
        domain: String,
        contentURL: URL
    ) -> ProductScriptSchemeHandler {
        let fileProvider = DotNsFileProvider(contentURL: contentURL)

        return ProductScriptSchemeHandler(
            productId: domain,
            entryRelativePath: ProductBundle.indexHTML,
            productFileProvider: fileProvider
        )
    }
}
