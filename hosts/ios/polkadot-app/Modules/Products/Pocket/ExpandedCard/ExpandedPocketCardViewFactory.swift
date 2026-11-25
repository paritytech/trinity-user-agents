import IssueMonitoring
import Products
import UIKit
import UIKitExt

enum ExpandedPocketCardViewFactory {
    @MainActor
    static func createView(
        card: PocketCardViewModel,
        page: ProductPage,
        contentSource: SPAContentSource,
        openTap: HostOpenTap? = nil,
        flowState: SPAFlowState
    ) -> ExpandedPocketCardViewProtocol? {
        guard let rust = SPAViewFactory.makeRustEnvironment(hostProvider: flowState.hostProvider) else { return nil }

        let configuration = SPAConfiguration(
            title: card.title,
            isRootScreen: false,
            showMoreButton: false,
            page: page,
            contentSource: contentSource,
            openTap: openTap
        )
        let routers = ProductRoutersFacade.spa()
        let schemeHandlerProxy = SchemeHandlerProxy()

        let runtimeFactory = ExpandedPocketCardRuntimeFactory(
            rust: rust,
            configuration: configuration,
            flowState: flowState,
            schemeHandlerProxy: schemeHandlerProxy,
            routers: routers
        )
        let interactor = ExpandedPocketCardInteractor(
            runtimeFactory: runtimeFactory,
            presenceRegistry: RootDependencyLocator.getDependency(),
            configuration: configuration,
            logger: Logger.shared
        )
        let presenter = ExpandedPocketCardPresenter(
            interactor: interactor,
            wireframe: ExpandedPocketCardWireframe(),
            hostProvider: flowState.hostProvider
        )
        let widgetView = WidgetView(schemeHandler: schemeHandlerProxy)
        let engine = SPAJSEngine(
            webView: widgetView.webView,
            navigationDecisionHandler: navigationDecisionHandler(
                for: configuration,
                hostProvider: flowState.hostProvider
            ),
            logger: Logger.shared
        )
        let view = ExpandedPocketCardViewController(
            presenter: presenter,
            card: card,
            productId: configuration.page.host.toDotDomain(),
            widgetView: widgetView,
            engine: engine
        )

        presenter.view = view
        interactor.presenter = presenter
        routers.setPresentationView(view)

        return view
    }
}

private extension ExpandedPocketCardViewFactory {
    static func navigationDecisionHandler(
        for configuration: SPAConfiguration,
        hostProvider: ProductHostProviding
    ) -> SPANavigationDecisionHandling {
        switch configuration.contentSource {
        case .dotNs:
            DotNsNavigationDecisionHandler(baseHost: configuration.page.host, hostProvider: hostProvider)
        case let .directURL(url):
            DirectURLNavigationDecisionHandler(baseURL: url)
        #if DEBUG
            case let .devServer(url, _):
                DirectURLNavigationDecisionHandler(baseURL: url)
        #endif
        }
    }
}

private final class ExpandedPocketCardRuntimeFactory: ExpandedPocketCardRuntimeMaking {
    let rust: RustRuntimeEnvironment
    let configuration: SPAConfiguration
    let flowState: SPAFlowState
    let schemeHandlerProxy: SchemeHandlerProxy
    let routers: ProductRoutersFacadeProtocol
    private var hostSession: SPARustRuntimeFactory.HostSession

    init(
        rust: RustRuntimeEnvironment,
        configuration: SPAConfiguration,
        flowState: SPAFlowState,
        schemeHandlerProxy: SchemeHandlerProxy,
        routers: ProductRoutersFacadeProtocol
    ) {
        self.rust = rust
        self.configuration = configuration
        self.flowState = flowState
        self.schemeHandlerProxy = schemeHandlerProxy
        self.routers = routers
        hostSession = SPARustRuntimeFactory.HostSession(openTap: configuration.openTap)
    }

    func makeRuntime(face: any ExpandedCardFaceShowing) throws -> SPARuntimeProtocol {
        let executionModel = try rust.makeExpandedCardExecution(
            productId: configuration.productId,
            routers: routers,
            face: face
        )

        // Taken once the execution opened, so a runtime that never got one is not an attempt.
        let attempt = hostSession.nextAttempt()

        return SPARustRuntime(
            executionModel: executionModel,
            configuration: configuration,
            productOpening: SPARustRuntime.ProductOpening(
                loader: flowState.contentLoader,
                strategy: .withAppFallback(.widget),
                resolver: flowState.productResolver
            ),
            schemeHandlerProxy: schemeHandlerProxy,
            logger: rust.logger,
            hostEventRecorder: IssueMonitoringFactory.createHostEventRecorder(),
            hostSessionID: hostSession.id,
            loadAttempt: attempt.number,
            openTap: attempt.openTap
        )
    }
}
