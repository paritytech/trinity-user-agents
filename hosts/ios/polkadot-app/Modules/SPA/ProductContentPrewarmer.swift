import Foundation
import Foundation_iOS
import Products
import ChainRegistry

@MainActor
protocol ProductContentPrewarming {
    func prewarm()
}

@MainActor
final class ProductContentPrewarmer {
    private let makeLabels: () async -> [String]
    private let chainRegistryClosure: ChainRegistryLazyClosure
    private let flowStateProvider: any SPAFlowStateProviding
    private let logger: LoggerProtocol

    private var prewarmTask: Task<Void, Never>?

    init(
        makeLabels: @escaping () async -> [String],
        chainRegistryClosure: @escaping ChainRegistryLazyClosure,
        flowStateProvider: any SPAFlowStateProviding,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.makeLabels = makeLabels
        self.chainRegistryClosure = chainRegistryClosure
        self.flowStateProvider = flowStateProvider
        self.logger = logger
    }
}

extension ProductContentPrewarmer: ProductContentPrewarming {
    func prewarm() {
        guard prewarmTask == nil else { return }

        // Captured strongly: the root module is released as soon as the launch decision lands, and
        // the warm has to outlive it.
        prewarmTask = Task {
            await warmContent()
            prewarmTask = nil
        }
    }
}

private extension ProductContentPrewarmer {
    func warmContent() async {
        await chainRegistryClosure().asyncWaitChainsSetup(for: [AppConfig.Chains.assethubChain])

        // Labels are resolved after the chain wait because they need the chain TLD and remote config.
        var seen = Set<String>()
        let labels = await makeLabels().filter { !$0.isEmpty && seen.insert($0).inserted }

        guard !labels.isEmpty else {
            logger.error("Product prewarm skipped: no labels")
            return
        }

        let flowState = flowStateProvider.flowState()

        for label in labels {
            await warm(label: label, flowState: flowState)
        }
    }

    func warm(label: String, flowState: SPAFlowState) async {
        guard let host = try? await flowState.hostProvider.resolveHost(label: label) else {
            logger.error("Product prewarm skipped: could not resolve TLD for \(label)")
            return
        }

        let domain = host.toDotDomain()

        do {
            // The app executable's archive, which is what the SPA loads.
            try await ProductArchiveWarmer(
                products: flowState.productResolver,
                dotNsResolver: flowState.dotNsResolver
            ).warm(domain, serving: .app)
            logger.debug("Product prewarm: warmed \(domain)")
        } catch {
            logger.error("Product prewarm: failed to warm \(domain): \(error)")
        }
    }
}
