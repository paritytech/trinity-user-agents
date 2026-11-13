import UIKit
import PolkadotUI
import TrUAPIHost

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

    func didRequestConnectionStatus() {
        wireframe.showConnectionStatus(connectionStatusViewModel, from: view)
    }

    func didRequestSettings() {
        wireframe.showSettings(from: view)
    }

    func didRequestBackupSettings() {
        wireframe.showBackupSettings(from: view)
    }

    func didFindChat(_ model: ChatOpenModel) {
        wireframe.openChat(model)
    }
}

extension MainTabBarPresenter: MainTabBarInteractorOutputProtocol {
    func didReceive(fundingService: FundingServiceProtocol) {
        wireframe.attachFunding(service: fundingService, view: view)
    }

    func didReceive(fundingEvent: FundingEvent) {
        switch fundingEvent {
        case let .presentationRequested(entry):
            wireframe.presentFunding(entry)
        case let .frameRequested(request):
            openWhenOnScreen(request)
        case let .presentFrame(frame):
            wireframe.presentProviderFrame(frame)
        case let .frameClosed(entryId):
            wireframe.closeProviderFrames(entryId: entryId)
        case let .providerNotWorking(providerId):
            wireframe.showProviderNotWorking(providerId)
        }
    }

    func didReceivePaymentsSetupWaiting(_ isWaiting: Bool) {
        wireframe.showPaymentsSetupNotice(isVisible: isWaiting)
    }

    func didReceiveRestoredBalanceWaiting(_ isWaiting: Bool) {
        view?.showSettingsAttention(isWaiting)
    }

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

private extension MainTabBarPresenter {
    /// Opens the screen a provider asked for at once while its session is on
    /// screen, and otherwise after the user taps the notice about it. A
    /// closed notice leaves the screen waiting in the core.
    func openWhenOnScreen(_ request: FundingFrameRequest) {
        if wireframe.isShowingFunding(entryId: request.entryId) {
            interactor.openProviderFrame(request)
        } else {
            wireframe.showProviderFrameNotice(for: request) { [interactor] in
                interactor.openProviderFrame(request)
            }
        }
    }
}
