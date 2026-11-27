import SwiftUI
import PolkadotUI

final class WalletMainViewController: SafeAreaFollowingHostingController<WalletView>, RootScreen, NavigationDependable,
    NavigationBarHiding {
    let presenter: WalletMainPresenterProtocol
    weak var navigationControlling: NavigationControlling?

    var hidesNavigationBar: Bool { true }

    private var assetDetailsScene: AssetDetailsScene
    private var isCollectiblesPressed = false

    init(presenter: WalletMainPresenterProtocol, assetDetailsScene: AssetDetailsScene) {
        self.presenter = presenter
        self.assetDetailsScene = assetDetailsScene
        super.init(content: WalletView())
    }

    @available(*, unavailable)
    @MainActor @preconcurrency dynamic required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func viewWillAppear(_ animated: Bool) {
        super.viewWillAppear(animated)

        guard content.viewModel.collectiblesPeek != .lifted else { return }

        withTransaction(Transaction(animation: nil)) {
            content.viewModel.collectiblesPeek = .resting
        }
    }

    override var bottomSafeAreaChange: BottomSafeAreaChange {
        if holdsCollectiblesPeek {
            return .held
        }
        return content.viewModel.openItem == nil ? .animated : .immediate
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .bgSurfaceMain
        navigationItem.style = .browser
        navigationControlling?.setNavigationBarHidden(hidesNavigationBar, animated: true)

        assetDetailsScene.attachNavigationHost(self)
        setupHandlers()
        presenter.setup()
    }

    private func setupHandlers() {
        content.viewModel.assetDetailsViewModel = assetDetailsScene.viewModel

        content.viewModel.onBalance = { [weak presenter] in
            presenter?.showBalanceCard()
        }

        content.viewModel.onCollectibles = { [weak self] in
            self?.openCollectibles()
        }

        content.viewModel.onCollectiblesPress = { [weak self] isPressed in
            self?.isCollectiblesPressed = isPressed
            self?.applySafeAreaInsets()
        }

        content.viewModel.onOpenPocketCard = { [weak presenter] card in
            presenter?.showPocketCard(card)
        }

        content.viewModel.onRemovePocketCard = { [weak presenter] card in
            presenter?.removePocketCard(card)
        }
    }
}

extension WalletMainViewController: WalletMainViewProtocol {
    func didReceive(isCollectiblesAvailable: Bool) {
        content.viewModel.isCollectiblesAvailable = isCollectiblesAvailable
    }

    func didReceive(pocketCards: [PocketCardViewModel]) {
        content.viewModel.pocketCards = pocketCards
    }

    /// Nil once the item is gone or the Wallet is not what closing the item
    /// returns to.
    func frame(of item: PocketItem) -> CGRect? {
        guard
            UIApplication.shared.mainTabBarController?.container.selectedController === navigationController,
            navigationController?.viewControllers.suffix(2).first === self
        else { return nil }

        // A return can ask before the Pocket has laid out on the Clearance it comes back to.
        view.layoutIfNeeded()

        switch item {
        case .balance:
            let frame = content.viewModel.balanceCardFrame
            return frame.isEmpty ? nil : frame
        case .collectibles:
            let frame = content.viewModel.collectiblesFrame
            return frame.isEmpty ? nil : frame
        case let .card(key):
            guard content.viewModel.pocketCards.contains(where: { $0.key == key }) else { return nil }

            return content.viewModel.pocketCardFrames[key]
        }
    }

    var openItem: PocketItem? {
        content.viewModel.openItem
    }

    func setOpenItem(_ item: PocketItem?) {
        let previous = content.viewModel.openItem
        content.viewModel.openItem = item

        if item == .collectibles {
            liftCollectiblesPeek()
        } else if previous == .collectibles {
            landCollectiblesPeek()
        }
    }

    func moveOpenItem(by offset: CGSize, clearance: CGFloat, animatedOver duration: TimeInterval?) {
        let viewModel = content.viewModel
        withTransaction(Transaction(animation: duration.map(PocketFlightTiming.animation(duration:)))) {
            viewModel.openPocketCardOffset = offset
            viewModel.pocketClearance = clearance
        }

        applySafeAreaInsets()
    }

    private func openCollectibles() {
        guard content.viewModel.collectiblesPeek == .resting else { return }

        withAnimation(.linear(duration: CollectiblesBookTiming.colourDuration)) {
            content.viewModel.collectiblesPeek = .colouring
        }

        Task { [weak self] in
            try? await Task.sleep(for: .seconds(CollectiblesBookTiming.colourDuration))
            self?.presenter.showCollectibles()
        }
    }
}

private extension WalletMainViewController {
    /// The peek keeps its place from the touch that opens it until its book has lifted out of it, so the Docklet
    /// tucking under that touch does not move it. The book flies from, and back to, the frame the peek reports.
    var holdsCollectiblesPeek: Bool {
        let viewModel = content.viewModel
        guard !viewModel.isPocketCleared else { return false }

        return isCollectiblesPressed || viewModel.collectiblesPeek != .resting
    }

    func liftCollectiblesPeek() {
        withTransaction(Transaction(animation: nil)) {
            content.viewModel.collectiblesPeek = .lifted
        }
    }

    func landCollectiblesPeek() {
        guard content.viewModel.collectiblesPeek == .lifted else { return }

        withTransaction(Transaction(animation: nil)) {
            content.viewModel.collectiblesPeek = .colouring
        }
        DispatchQueue.main.async { [weak self] in
            withAnimation(.linear(duration: CollectiblesBookTiming.colourDuration)) {
                self?.content.viewModel.collectiblesPeek = .resting
            }
            self?.applySafeAreaInsets()
        }
    }
}
