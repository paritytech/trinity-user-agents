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
        flowState: SPAFlowState
    ) -> ExpandedPocketCardViewProtocol? {
        guard let rust = SPAViewFactory.makeRustEnvironment(hostProvider: flowState.hostProvider) else { return nil }

        let configuration = SPAConfiguration(
            title: card.title,
            isRootScreen: false,
            showMoreButton: false,
            page: page,
            contentSource: contentSource
        )
        let routers = ProductRoutersFacade.spa()
        let schemeHandlerProxy = SchemeHandlerProxy()
        // Portrait only, so the face's bottom edge is fixed for the screen's life.
        let cardInset = (UIWindow.keyWindow?.safeAreaInsets.top ?? 0) + PocketOpenedCardView.height

        let runtimeFactory = ExpandedPocketCardRuntimeFactory(
            rust: rust,
            configuration: configuration,
            flowState: flowState,
            schemeHandlerProxy: schemeHandlerProxy,
            routers: routers,
            cardInset: cardInset
        )
        let interactor = ExpandedPocketCardInteractor(
            runtimeFactory: runtimeFactory,
            presenceRegistry: RootDependencyLocator.getDependency(),
            configuration: configuration,
            logger: Logger.shared
        )
        let presenter = ExpandedPocketCardPresenter(interactor: interactor, wireframe: ExpandedPocketCardWireframe())
        let view = ExpandedPocketCardViewController(
            presenter: presenter,
            cardInset: cardInset,
            schemeHandlerProxy: schemeHandlerProxy
        )

        presenter.view = view
        interactor.presenter = presenter
        routers.setPresentationView(view)

        return view
    }
}

private struct ExpandedPocketCardRuntimeFactory: ExpandedPocketCardRuntimeMaking {
    let rust: RustRuntimeEnvironment
    let configuration: SPAConfiguration
    let flowState: SPAFlowState
    let schemeHandlerProxy: SchemeHandlerProxy
    let routers: ProductRoutersFacadeProtocol
    let cardInset: CGFloat

    func makeRuntime(face: any ExpandedCardFaceShowing) throws -> SPARuntimeProtocol {
        let executionModel = try rust.makeExpandedCardExecution(
            productId: configuration.productId,
            routers: routers,
            face: face
        )

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
            pageScripts: [.pocketCardInset(cardInset)]
        )
    }
}
