import UIKit
import PolkadotUI

@MainActor
final class MainTabBarPresenter {
    weak var view: MainTabBarViewProtocol?
    let wireframe: MainTabBarWireframeProtocol
    let interactor: MainTabBarInteractorInputProtocol

    /// `.spaTabs` is dropped by the chrome while no apps are open; it is declared here so its
    /// position next to `.scan` is owned by the slot list rather than by insertion order.
    let slots: [TabBarSlot] = [
        .tab(.chat), .tab(.wallet), .action(.scan), .action(.spaTabs),
        .tab(.browse), .tab(.settings)
    ]

    private let chipViewModelFactory: SPATabChipViewModelFactory
    private var chainStatusRows: [ChainConnectionStatusViewModel] = []

    init(
        interactor: MainTabBarInteractorInputProtocol,
        wireframe: MainTabBarWireframeProtocol,
        chipViewModelFactory: SPATabChipViewModelFactory
    ) {
        self.interactor = interactor
        self.wireframe = wireframe
        self.chipViewModelFactory = chipViewModelFactory
    }
}

extension MainTabBarPresenter: MainTabBarPresenterProtocol {
    func setup() {
        interactor.setup()
    }

    func configureViews() {
        view?.show(slots: slots, selecting: .chat)
    }

    func didRequestContentPanel(for action: TabBarAction) {
        switch action {
        case .scan:
            view?.showScanPanel()
        case .spaTabs:
            break
        case .connectionStatus:
            showConnectionStatusPanel()
        }
    }

    #if FEATURE_INPUT
        func didFindChat(_ model: ChatOpenModel) {
            wireframe.openChat(model)
        }
    #else
        func didRequestContactSearch() {
            wireframe.showSearchContact(from: view)
        }
    #endif
}

extension MainTabBarPresenter: MainTabBarInteractorOutputProtocol {
    func didReceiveWidget(
        configuration: any HashableContentConfiguration,
        for extensionId: ChatExtension.Id
    ) {
        view?.attachWidget(
            configuration,
            for: AppWidgetID(extensionId)
        )
    }

    func didRemoveWidget(for extensionId: ChatExtension.Id) {
        view?.detachWidget(for: AppWidgetID(extensionId))
    }

    func didReceivePolkadotSignInRequest(with url: URL) {
        wireframe.showPolkadotSignIn(with: url, view: view)
    }

    func didReceiveSPATabs(_ tabs: [SPATab]) {
        view?.showSPATabs(chipViewModelFactory.createViewModels(for: tabs))
    }

    func didReceiveChainStatus(_ rows: [ChainConnectionStatusViewModel]) {
        view?.showChainStatus(rows)

        chainStatusRows = rows
        showConnectionStatusPanel()
    }

    func didReceiveTabBarLabelsEnabled(_ isEnabled: Bool) {
        view?.setLabels(visible: isEnabled)
    }

    func didReceiveProductGamePill(_ pill: ProductGamePill?) {
        let viewModel = pill.map { pill in
            ProductGamePillViewModel(
                productId: pill.productId,
                configuration: GameRoomPillConfiguration(content: .waiting(gameDate: pill.startsAt)) { [wireframe] in
                    wireframe.openProduct(productId: pill.productId)
                }
            )
        }
        view?.showProductGamePill(viewModel)
    }
}

private extension MainTabBarPresenter {
    /// `setContentPanel` ignores pushes unless its panel is open, so this is a no-op while closed.
    func showConnectionStatusPanel() {
        view?.showTabBarPanelContent(
            SwiftUIContentConfiguration(view: ConnectionStatusPanelView(rows: chainStatusRows)),
            for: .connectionStatus
        )
    }
}
