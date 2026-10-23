import Foundation
import Operation_iOS
import Products

/// Rust-runtime SPA interactor: creates the runtime via the factory, attaches
/// the presentation view, and forwards the start URL to the presenter. All
/// setup logic lives in the runtime itself.
@MainActor
final class SPARustRuntimeInteractor {
    weak var presenter: SPAInteractorOutputProtocol?

    private let runtimeFactory: SPARuntimeFactoryProtocol
    private let productResolver: ProductResolving
    private let contentLoader: ProductContentLoading
    private let presenceRegistry: ProductPresenceRegistering?
    private let configuration: SPAConfiguration
    private let logger: LoggerProtocol
    private let productRepository: AnyDataProviderRepository<Product>
    private let chatProviderFactory: ChatContactDataProviderMaking

    private var runtime: SPARuntimeProtocol?
    private var jsEngine: JSEngineProtocol?
    // Held so the synchronous `hasChatEntry` can answer from the manifest without re-resolving.
    private var resolvedProduct: ResolvedProduct?
    private var servedContent: DotNsResolvedContent?
    private var isServing = false
    private var presenceId: UUID?

    private var setupTask: Task<Void, Never>?
    private var openChatTask: Task<Void, Never>?

    init(
        runtimeFactory: SPARuntimeFactoryProtocol,
        productResolver: ProductResolving,
        contentLoader: ProductContentLoading,
        presenceRegistry: ProductPresenceRegistering?,
        configuration: SPAConfiguration,
        logger: LoggerProtocol,
        productRepository: AnyDataProviderRepository<Product>,
        chatProviderFactory: ChatContactDataProviderMaking
    ) {
        self.runtimeFactory = runtimeFactory
        self.productResolver = productResolver
        self.contentLoader = contentLoader
        self.presenceRegistry = presenceRegistry
        self.configuration = configuration
        self.logger = logger
        self.productRepository = productRepository
        self.chatProviderFactory = chatProviderFactory
    }

    deinit {
        setupTask?.cancel()
        openChatTask?.cancel()
        // Runtime ownership contract: the holder disposes before release.
        Task { [runtime] in await runtime?.dispose() }

        if let presenceId, let presenceRegistry {
            Task { @MainActor in presenceRegistry.unregister(presenceId) }
        }
    }
}

extension SPARustRuntimeInteractor: SPAInteractorInputProtocol {
    func setup(engine: JSEngineProtocol) {
        jsEngine = engine

        startRuntime(engine: engine)
    }

    func retry() {
        guard let engine = jsEngine else { return }

        let previousRuntime = runtime
        runtime = nil
        Task { await previousRuntime?.dispose() }

        startRuntime(engine: engine)
    }

    func reloadLatestBuild() {
        guard isServing else {
            presenter?.didRestartLoading()
            retry()
            return
        }

        guard servedContent != nil else {
            presenter?.didRequestReload()
            return
        }

        guard presenceRegistry?.applyPendingUpdate(of: productDomain) != true else { return }

        serveAppliedBuild()
    }

    func checkForNewBuild() {
        guard servedContent != nil else { return }

        presenceRegistry?.requestRefresh(of: productDomain)
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
                self?.logger.error("SPA(rust): Failed to open chat: \(error)")
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

private extension SPARustRuntimeInteractor {
    /// A dotNS page is its manifest, so a resolution failure is the page failing. A direct URL is
    /// its own content and the manifest only names it, so a label that holds no records — a local
    /// server opened from debug settings — still opens, unnamed. A development server page is not
    /// published content, so nothing is resolved for it.
    func resolveProduct() async throws -> ResolvedProduct? {
        let domain = configuration.page.host.toDotDomain()

        switch configuration.contentSource {
        case .dotNs:
            return try await contentLoader.open(domain, policy: .untilCancelled, onPhase: phaseHandler()).resolved
        case .directURL:
            return try? await productResolver.resolve(domain)
        #if DEBUG
            case .devServer:
                return nil
        #endif
        }
    }

    func startRuntime(engine: JSEngineProtocol) {
        setupTask?.cancel()
        isServing = false
        servedContent = nil

        do {
            let newRuntime = try runtimeFactory.createRuntime(for: configuration.productId)
            runtime = newRuntime

            setupTask = Task { [weak self] in
                do {
                    self?.resolvedProduct = try await self?.resolveProduct()

                    let url = try await newRuntime.start(with: engine)
                    self?.presenter?.didRequestNavigation(to: url)
                    await self?.didStartServing(runtime: newRuntime)
                } catch is CancellationError {
                    // Superseded by retry/teardown — the replacement reports.
                } catch {
                    self?.logger.error("SPA(rust): Setup failed: \(error)")
                    self?.presenter?.didFail(error: error)
                }
            }
        } catch {
            logger.error("SPA(rust): Runtime creation failed: \(error)")
            presenter?.didFail(error: error)
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
}

extension SPARustRuntimeInteractor: SPAProductReloading {
    func serveAppliedBuild() {
        Task { [weak self, runtime] in
            do {
                try await runtime?.useLatestBuild()
                self?.servedContent = await runtime?.servedBuild()
                self?.presenter?.didRequestReload()
            } catch {
                self?.logger.error("SPA(rust): Failed to serve the applied build: \(error)")
            }
        }
    }
}

private extension SPARustRuntimeInteractor {
    var productDomain: ProductId {
        configuration.page.host.toDotDomain()
    }

    func phaseHandler() -> ProductLoadPhaseHandler {
        { [weak self] phase in
            Task { @MainActor in self?.presenter?.didUpdateLoadPhase(phase) }
        }
    }

    func didStartServing(runtime: SPARuntimeProtocol) async {
        isServing = true
        servedContent = await runtime.servedBuild()

        guard servedContent != nil, presenceId == nil else { return }

        presenceId = presenceRegistry?.register(productId: productDomain, reloader: self)
    }
}
