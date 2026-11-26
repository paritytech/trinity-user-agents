import Foundation
import Keystore_iOS
import Operation_iOS
import Products
import KeyDerivation
import Individuality
import SubstrateSdk
import SubstrateStorageQuery
import ChainRegistry
import BulletinChain
import TrUAPIHost
import DesignSystem

enum SPAViewFactory {
    @MainActor
    static func createView(
        configuration: SPAConfiguration,
        flowState: SPAFlowState
    ) -> SPAViewProtocol? {
        let routers = ProductRoutersFacade.spa()
        let wireframe = SPAWireframe()
        let schemeHandlerProxy = SchemeHandlerProxy()

        guard let rustEnvironment = makeRustEnvironment(hostProvider: flowState.hostProvider) else { return nil }

        let runtimeFactory = SPARustRuntimeFactory(environment: .init(
            rust: rustEnvironment,
            configuration: configuration,
            contentLoader: flowState.contentLoader,
            productResolver: flowState.productResolver,
            schemeHandlerProxy: schemeHandlerProxy,
            routers: routers
        ))

        let interactor = SPARustRuntimeInteractor(
            runtimeFactory: runtimeFactory,
            presenceRegistry: RootDependencyLocator.getDependency(),
            configuration: configuration,
            logger: Logger.shared,
            productRepository: ProductRepositoryFactory().createRepository(),
            chatProviderFactory: ChatContactDataProviderFactory(),
            chainStatusProvider: chainStatusProvider(for: configuration)
        )

        let presenter = SPAPresenter(
            interactor: interactor,
            wireframe: wireframe,
            configuration: configuration,
            hostProvider: flowState.hostProvider
        )

        let view = createViewController(
            presenter: presenter,
            configuration: configuration,
            schemeHandlerProxy: schemeHandlerProxy,
            hostProvider: flowState.hostProvider
        )

        presenter.view = view
        interactor.presenter = presenter
        runtimeFactory.setPresentationView(view)

        if !configuration.isRootScreen {
            view.controller.hidesBottomBarWhenPushed = true
        }

        return view
    }

    @MainActor
    static func createView(
        page: ProductPage,
        flowState: SPAFlowState,
        presentation: SPAPresentation = .pushed,
        browserTabId: UUID? = nil
    ) -> SPAViewProtocol? {
        let configuration = SPAConfiguration(
            title: nil,
            isRootScreen: false,
            showMoreButton: true,
            page: page,
            presentation: presentation,
            browserTabId: browserTabId
        )

        return createView(
            configuration: configuration,
            flowState: flowState
        )
    }
}

extension SPAViewFactory {
    /// The shared ``TrUAPIHostRuntime`` is sourced from the process-wide provider registered by
    /// `ServiceCoordinator`; nil, logged, when the session has none.
    @MainActor
    static func makeRustEnvironment(hostProvider: ProductHostProviding) -> RustRuntimeEnvironment? {
        guard let runtimeProvider: TrUAPIHostRuntimeProviding = RootDependencyLocator.getDependency() else {
            Logger.shared.error("Rust runtime unavailable: TrUAPI runtime provider missing")
            return nil
        }

        let runtime: TrUAPIHostRuntime
        do {
            runtime = try runtimeProvider.sharedRuntime()
        } catch {
            Logger.shared.error("Rust runtime unavailable: \(error)")
            return nil
        }

        return RustRuntimeEnvironment(
            runtime: runtime,
            chainRegistry: ChainRegistryFacade.sharedRegistry,
            notificationScheduler: ProductNotificationScheduler.shared,
            gameReminders: RootDependencyLocator.getDependency(),
            ipfsFetcher: IpfsFetcher(ipfsBaseURL: AppConfig.KnownIPFS.main),
            hostProvider: hostProvider,
            themeManager: ThemeManager.shared,
            logger: Logger.shared
        )
    }
}

// MARK: - Private

private extension SPAViewFactory {
    /// Sheets get the simplified screen, since browser chrome (pills, collapse, minimize) only suits a mounted tab.
    @MainActor
    static func createViewController(
        presenter: SPAPresenterProtocol,
        configuration: SPAConfiguration,
        schemeHandlerProxy: SchemeHandlerProxy,
        hostProvider: ProductHostProviding
    ) -> SPAViewProtocol {
        switch configuration.presentation {
        case .sheet,
             .fullScreen:
            let controller = SPASimplifiedViewController(
                presenter: presenter,
                configuration: configuration,
                schemeHandlerProxy: schemeHandlerProxy,
                logger: Logger.shared,
                hostProvider: hostProvider
            )
            controller.modalPresentationStyle = configuration.presentation == .fullScreen ? .fullScreen : .pageSheet
            return controller
        case .pushed,
             .browserTab:
            return SPAViewController(
                presenter: presenter,
                configuration: configuration,
                schemeHandlerProxy: schemeHandlerProxy,
                logger: Logger.shared,
                hostProvider: hostProvider
            )
        }
    }

    /// Only the full-screen product shows the chain status strip, so only it gets the provider.
    @MainActor
    static func chainStatusProvider(for configuration: SPAConfiguration) -> ChainStatusProviding? {
        guard
            configuration.presentation == .fullScreen,
            let chainStatusProvider: ChainStatusProviding = RootDependencyLocator.getDependency()
        else {
            return nil
        }

        return chainStatusProvider
    }
}
