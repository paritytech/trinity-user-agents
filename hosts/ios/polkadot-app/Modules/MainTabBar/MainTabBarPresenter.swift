import UIKit
import PolkadotUI

@MainActor
final class MainTabBarPresenter {
    weak var view: MainTabBarViewProtocol?
    let wireframe: MainTabBarWireframeProtocol
    let interactor: MainTabBarInteractorInputProtocol

    /// The four visible slots, left to right. The centre control is not a slot.
    #if FEATURE_PRODUCTS
        let slots: [TabBarSlot] = [.tab(.chat), .tab(.wallet), .tab(.browse), .action(.spaTabs)]
    #else
        let slots: [TabBarSlot] = [.tab(.chat), .tab(.wallet), .tab(.settings), .action(.connectionStatus)]
    #endif

    private let chipViewModelFactory: SPATabChipViewModelFactory
    private let connectionStatusViewModel = ConnectionStatusSheetViewModel()
    #if !FEATURE_PRODUCTS
        private var isSettingsAttentionVisible = false
    #endif

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
        #if !FEATURE_PRODUCTS
            view?.setAttention(isSettingsAttentionVisible, for: .settings)
        #endif
    }

    func didRequestConnectionStatus() {
        wireframe.showConnectionStatus(connectionStatusViewModel, from: view)
    }

    func didRequestSettings() {
        wireframe.showSettings(from: view)
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
    #if !FEATURE_PRODUCTS
        func didUpdateSettingsAttention(isVisible: Bool) {
            guard isSettingsAttentionVisible != isVisible else {
                return
            }
            isSettingsAttentionVisible = isVisible
            view?.setAttention(isVisible, for: .settings)
        }
    #endif

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
        connectionStatusViewModel.rows = rows
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
