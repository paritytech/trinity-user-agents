import Foundation
import Keystore_iOS
import CryptoKit
import Products
import TrUAPIHost
import UIKitExt
import UIKit
import ChainRegistry
import SubstrateSdk

final class TrUAPIWorkerEngines: WorkerEngineHost, @unchecked Sendable {
    private struct Engine {
        let runtime: ChatRustRuntime
        let bot: ProductBot
        let task: Task<Void, Error>
    }

    private struct BundleManifest: Codable {
        let executable: String?
        let entrypoint: String
    }

    private struct CachedBundle: Codable {
        let contentId: String
        let manifest: Data
    }

    private let resolver: DotNsResolverProtocol
    private let logger: LoggerProtocol
    private weak var host: TrUAPIHostRuntime?
    private let lock = NSLock()
    private var surfaces: [String: ProductChatSurface] = [:]
    private var engines: [String: Engine] = [:]
    private var changes: [UUID: AsyncStream<Void>.Continuation] = [:]
    private var observers: [NSObjectProtocol] = []

    init(host: TrUAPIHostRuntime, resolver: DotNsResolverProtocol, logger: LoggerProtocol) {
        self.resolver = resolver
        self.logger = logger
        self.host = host
        observers.append(NotificationCenter.default.addObserver(
            forName: UIApplication.willResignActiveNotification, object: nil, queue: nil
        ) { [weak self] _ in
            guard let self else { return }
            Task.detached { [weak self] in
                do {
                    try await self?.host?.suspendWorkers()
                } catch {
                    self?.logger.error("Rust worker suspension failed: \(error)")
                }
            }
        })
        observers.append(NotificationCenter.default.addObserver(
            forName: UIApplication.didBecomeActiveNotification, object: nil, queue: nil
        ) { [weak self] _ in
            Task { [weak self] in
                do {
                    try await self?.host?.updateWorkers()
                } catch {
                    self?.logger.error("Rust worker restoration failed: \(error)")
                }
            }
        })
    }

    deinit { observers.forEach(NotificationCenter.default.removeObserver) }

    func chatBridge(productId: String) -> (any ChatHostBridge)? {
        let surface = lock.withLock {
            if let surface = surfaces[productId] { return surface }
            let surface = ProductChatSurface()
            surfaces[productId] = surface
            return surface
        }
        return RustChatExecutionBridge(chatMessaging: surface, logger: logger)
    }

    func pocketBridge(productId _: String) -> (any PocketHostBridge)? { nil }

    func fetchWorkerBundle(productId: String, contentHash: Data?) async throws -> WorkerBundle {
        let metadataDirectory = try FileManager.default.url(
            for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true
        )
            .appendingPathComponent("TrUAPIWorkerBundles", isDirectory: true)
        try FileManager.default.createDirectory(at: metadataDirectory, withIntermediateDirectories: true)
        if let contentHash {
            let cached = try JSONDecoder().decode(
                CachedBundle.self,
                from: Data(contentsOf: metadataDirectory.appendingPathComponent(contentHash.toHex()))
            )
            guard let directory = DotNsContentStorage().getContentDirectory(contentHash: cached.contentId) else {
                throw HostRejection.Rejected(reason: "Worker bundle files are missing")
            }
            return WorkerBundle(contentHash: contentHash, manifest: cached.manifest, localPath: directory.path)
        }
        let subname = ProductManifestRecords.subname(base: productId, kind: .worker)
        let text = try await resolver.getMetadataEntry(dotNsName: subname, key: ProductManifestRecords.executableKey)
        let source: ProductWorkerSource
        if let text {
            guard case let .worker(worker)? = ProductManifestParser(logger: logger)
                .parseExecutable(text, kind: .worker, identifier: subname) else {
                throw HostRejection.Rejected(reason: "Product has no valid worker manifest")
            }
            source = ProductWorkerSource(contentId: subname, entryRelativePath: worker.entrypoint)
        } else {
            guard let host else { throw HostRejection.Rejected(reason: "Worker runtime stopped") }
            let workerOverride = try await host.products().first { $0.productId == productId }?.workerUrlOverride
            guard let workerOverride else { throw HostRejection.Rejected(reason: "Product has no worker executable") }
            let url = URL(string: workerOverride.contains("://") ? workerOverride : "https://" + workerOverride)
            guard let domain = url?.host,
                  let productHost = try await ProductHostFactory(tldProvider: DotNsTldProviderFacade.shared)
                    .resolveHost(rawString: domain),
                  let path = url?.path, !path.isEmpty else {
                throw HostRejection.Rejected(
                    reason: "Arbitrary HTTP development worker bundles are unsupported in this experiment; "
                        + "use a published dotNS bundle"
                )
            }
            source = ProductWorkerSource(
                contentId: productHost.toDotDomain(), entryRelativePath: String(path.drop(while: { $0 == "/" }))
            )
        }
        let directory = try await resolver.resolveToLocalURL(dotNsName: source.contentId)
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        let manifest = try encoder.encode(BundleManifest(executable: text, entrypoint: source.entryRelativePath))
        let cached = CachedBundle(contentId: directory.lastPathComponent, manifest: manifest)
        let metadata = try encoder.encode(cached)
        let hash = Data(SHA256.hash(data: metadata))
        try metadata.write(to: metadataDirectory.appendingPathComponent(hash.toHex()), options: .atomic)
        return WorkerBundle(contentHash: hash, manifest: manifest, localPath: directory.path)
    }

    func startWorker(productId: String, execution: TrUAPIProductExecution, bundle: WorkerBundle) throws {
        let manifest = try JSONDecoder().decode(BundleManifest.self, from: bundle.manifest)
        let files = WorkerFiles(directory: URL(fileURLWithPath: bundle.localPath))
        let context = try ChatProductEngineFactory.makeContext(
            source: ProductWorkerSource(contentId: productId, entryRelativePath: manifest.entrypoint),
            productFileProvider: files,
            logger: logger
        )
        let surface = lock.withLock { surfaces[productId] ?? ProductChatSurface() }
        let routers = ProductRoutersFacade.worker()
        let connections = TrUAPIChainConnectionPool(engineResolver: { _ in nil }, logger: logger)
        let permissions = OSPermissionAsker()
        let runtime = ChatRustRuntime(
            productUrl: context.productUrl,
            makeExecutionModel: { _ in
                RustRuntimeEnvironment.ExecutionModel(
                    execution: execution, chainConnections: connections, osPermissionAsker: permissions
                )
            },
            routers: routers,
            engineFactory: context.engineFactory,
            chatSurface: surface,
            logger: logger
        )
        let bot = ProductBot(product: Product(id: productId, name: productId), runtime: runtime, logger: logger)
        let messaging = ChatExtensionDiscoverContext(
            settings: SettingsManager.shared,
            storageFacade: UserDataStorageFacade.shared,
            operationQueue: OperationManagerFacade.sharedDefaultQueue,
            logger: logger
        )
        let task = Task<Void, Error> { [weak self] in
            do {
                try await runtime.start(messagingSupport: .init(bot: bot, context: messaging))
            } catch {
                if !(error is CancellationError) {
                    self?.host?.notifyWorkerFailed(execution: execution, reason: String(describing: error))
                }
                throw error
            }
        }
        let listeners = lock.withLock {
            engines[productId] = Engine(runtime: runtime, bot: bot, task: task)
            return Array(changes.values)
        }
        listeners.forEach { $0.yield(()) }
    }

    func stopWorker(productId: String) async throws {
        let engine = lock.withLock {
            surfaces.removeValue(forKey: productId)
            return engines.removeValue(forKey: productId)
        }
        engine?.task.cancel()
        if let engine {
            await engine.runtime.dispose()
            _ = try? await engine.task.value
        }
    }

    func chatRuntime(productId: String) async throws -> ChatRustRuntime {
        let identifier = UUID()
        let updates = AsyncStream<Void>(bufferingPolicy: .bufferingNewest(1)) { continuation in
            lock.withLock { changes[identifier] = continuation }
            continuation.yield(())
        }
        defer { _ = lock.withLock { changes.removeValue(forKey: identifier) } }
        for await _ in updates {
            try Task.checkCancellation()
            if let engine = lock.withLock({ engines[productId] }) {
                try await engine.task.value
                try Task.checkCancellation()
                return engine.runtime
            }
        }
        throw CancellationError()
    }
}

private struct WorkerFiles: ChatProductFileProviding {
    let directory: URL
    func manualScriptEntryPath(productId _: ProductId) -> String? { nil }
    func load(for _: ProductId, relativePath: String) -> Data? {
        let root = directory.resolvingSymlinksInPath().standardizedFileURL
        let file = root.appendingPathComponent(relativePath).resolvingSymlinksInPath().standardizedFileURL
        guard file.path.hasPrefix(root.path + "/") else { return nil }
        return try? Data(contentsOf: file)
    }
}

final class TrUAPIWorkerChatRuntime: ChatRuntimeProtocol, @unchecked Sendable {
    private let productId: String
    private let engines: TrUAPIWorkerEngines
    private let lock = NSLock()
    private var presentationTask: Task<Void, Never>?

    init(productId: String, engines: TrUAPIWorkerEngines) {
        self.productId = productId
        self.engines = engines
    }

    func start(messagingSupport _: ProductsNativeApi.MessagingSupport) async throws {}
    deinit { presentationTask?.cancel() }

    func dispose() async {
        lock.withLock {
            presentationTask?.cancel()
            presentationTask = nil
        }
    }
    func onUserMessage(text: String, roomId: String?) async throws {
        try await engines.chatRuntime(productId: productId).onUserMessage(text: text, roomId: roomId)
    }
    func renderMessage(
        roomId: String?, messageId: String, messageType: String, messageData: Data
    ) async -> AsyncThrowingStream<ChatRendererOutput, Error> {
        do {
            return try await engines.chatRuntime(productId: productId).renderMessage(
                roomId: roomId, messageId: messageId, messageType: messageType, messageData: messageData
            )
        } catch {
            return AsyncThrowingStream { $0.finish(throwing: error) }
        }
    }
    func dispatchEvent(
        roomId: String?, messageId: String, messageType: String?, actionId: String, payload: String?
    ) async {
        do {
            try await engines.chatRuntime(productId: productId).dispatchEvent(
                roomId: roomId, messageId: messageId, messageType: messageType, actionId: actionId, payload: payload
            )
        } catch {
            Logger.shared.error("Worker event failed: \(error)")
        }
    }
    @MainActor func attach(presentationView: ControllerBackedProtocol) {
        lock.withLock {
            presentationTask?.cancel()
            presentationTask = Task { @MainActor [engines, productId] in
                do {
                    try await engines.chatRuntime(productId: productId).attach(presentationView: presentationView)
                } catch is CancellationError {
                } catch {
                    Logger.shared.error("Worker presentation failed: \(error)")
                }
            }
        }
    }
}
