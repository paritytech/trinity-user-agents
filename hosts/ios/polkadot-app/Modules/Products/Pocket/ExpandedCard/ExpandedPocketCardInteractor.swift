import Foundation
import Products
import enum TrUAPIHost.ExpandedCardFaceOutcome

@MainActor
final class ExpandedPocketCardInteractor {
    weak var presenter: ExpandedPocketCardInteractorOutputProtocol?

    private let runtimeFactory: ExpandedPocketCardRuntimeMaking
    private let presenceRegistry: ProductPresenceRegistering?
    private let configuration: SPAConfiguration
    private let logger: LoggerProtocol

    private var engine: JSEngineProtocol?
    private var cardInset: CGFloat = 0
    private var runtime: SPARuntimeProtocol?
    private var servedContent: ProductContent?
    private var displayName: String?
    private var presenceId: UUID?

    private var setupTask: Task<Void, Never>?
    private var reloadTask: Task<Void, Never>?

    init(
        runtimeFactory: ExpandedPocketCardRuntimeMaking,
        presenceRegistry: ProductPresenceRegistering?,
        configuration: SPAConfiguration,
        logger: LoggerProtocol
    ) {
        self.runtimeFactory = runtimeFactory
        self.presenceRegistry = presenceRegistry
        self.configuration = configuration
        self.logger = logger
    }

    deinit {
        setupTask?.cancel()
        reloadTask?.cancel()
        Task { [runtime] in await runtime?.dispose() }

        if let presenceId, let presenceRegistry {
            Task { @MainActor in presenceRegistry.unregister(presenceId) }
        }
    }
}

extension ExpandedPocketCardInteractor: ExpandedPocketCardInteractorInputProtocol {
    func setup(engine: JSEngineProtocol, cardInset: CGFloat) {
        self.engine = engine
        self.cardInset = cardInset
        startRuntime(engine: engine)
    }

    func retry() {
        guard let engine else { return }

        reloadTask?.cancel()
        let previousRuntime = runtime
        runtime = nil
        Task { await previousRuntime?.dispose() }

        startRuntime(engine: engine)
    }

    func checkForNewBuild() {
        guard servedContent != nil else { return }

        presenceRegistry?.requestRefresh(of: configuration.productDomain)
    }
}

extension ExpandedPocketCardInteractor: ExpandedCardFaceShowing {
    func setFaceShown(_ shown: Bool) -> ExpandedCardFaceOutcome {
        presenter?.didRequestFaceShown(shown) ?? .notPresented
    }
}

extension ExpandedPocketCardInteractor: SPAProductReloading {
    var servedContentHash: String? {
        servedContent?.archive.contentHash
    }

    func serveAppliedBuild() {
        reloadTask?.cancel()
        reloadTask = Task { [weak self, runtime] in
            do {
                let served = try await runtime?.useLatestBuild()

                guard !Task.isCancelled else { return }

                self?.didServe(served)
                self?.presenter?.didRequestReload()
            } catch {
                guard !Task.isCancelled else { return }

                self?.logger.error("Expanded card: failed to serve the applied build: \(error)")
                self?.presenter?.didFail(error: error)
            }
        }
    }
}

private extension ExpandedPocketCardInteractor {
    func startRuntime(engine: JSEngineProtocol) {
        setupTask?.cancel()
        servedContent = nil

        do {
            let newRuntime = try runtimeFactory.makeRuntime(face: self, cardInset: cardInset)
            runtime = newRuntime

            let onPhase = phaseHandler()
            setupTask = Task { [weak self] in
                do {
                    let start = try await newRuntime.start(with: engine, onPhase: onPhase)
                    self?.displayName = start.resolution.product?.displayName
                    self?.presenter?.didRequestNavigation(to: start.url)
                    self?.didServe(start.resolution.content)
                } catch is CancellationError {
                    // Superseded by a retry or teardown, which reports for itself.
                } catch {
                    self?.logger.error("Expanded card: setup failed: \(error)")
                    self?.presenter?.didFail(error: error)
                }
            }
        } catch {
            logger.error("Expanded card: runtime creation failed: \(error)")
            presenter?.didFail(error: error)
        }
    }

    func phaseHandler() -> ProductLoadPhaseHandler {
        { [weak self] phase in
            Task { @MainActor in self?.presenter?.didUpdateLoadPhase(phase) }
        }
    }

    func didServe(_ content: ProductContent?) {
        servedContent = content

        guard presenceId == nil, let contentId = try? content?.contentId else { return }

        presenceId = presenceRegistry?.register(
            productId: configuration.productDomain,
            contentId: contentId,
            displayName: displayName ?? configuration.page.host.name,
            reloader: self
        )
    }
}
