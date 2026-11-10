import SwiftUI

final class WalletMainViewController: UIHostingController<WalletView>, RootScreen, NavigationDependable,
    NavigationBarHiding {
    let presenter: WalletMainPresenterProtocol
    weak var navigationControlling: NavigationControlling?

    var hidesNavigationBar: Bool { true }

    private var assetDetailsScene: AssetDetailsScene

    init(presenter: WalletMainPresenterProtocol, assetDetailsScene: AssetDetailsScene) {
        self.presenter = presenter
        self.assetDetailsScene = assetDetailsScene
        super.init(rootView: WalletView())
    }

    @available(*, unavailable)
    @MainActor @preconcurrency dynamic required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func viewWillAppear(_ animated: Bool) {
        super.viewWillAppear(animated)

        guard rootView.viewModel.collectiblesPeek != .lifted else { return }

        withTransaction(Transaction(animation: nil)) {
            rootView.viewModel.collectiblesPeek = .resting
        }
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
        rootView.viewModel.assetDetailsViewModel = assetDetailsScene.viewModel

        rootView.viewModel.onBalance = { [weak presenter] in
            presenter?.showBalanceCard()
        }

        rootView.viewModel.onCollectibles = { [weak self] in
            self?.openCollectibles()
        }

        rootView.viewModel.onOpenPocketCard = { [weak presenter] card in
            presenter?.showPocketCard(card)
        }

        rootView.viewModel.onRemovePocketCard = { [weak presenter] card in
            presenter?.removePocketCard(card)
        }
    }
}

extension WalletMainViewController: WalletMainViewProtocol {
    func didReceive(isCollectiblesAvailable: Bool) {
        rootView.viewModel.isCollectiblesAvailable = isCollectiblesAvailable
    }

    func didReceive(pocketCards: [PocketCardViewModel]) {
        rootView.viewModel.pocketCards = pocketCards
    }

    /// Nil once the item is gone or the Wallet is not what closing the item
    /// returns to.
    func frame(of item: PocketItem) -> CGRect? {
        guard
            UIApplication.shared.mainTabBarController?.container.selectedController === navigationController,
            navigationController?.viewControllers.suffix(2).first === self
        else { return nil }

        switch item {
        case .balance:
            let frame = rootView.viewModel.balanceCardFrame
            return frame.isEmpty ? nil : frame
        case .collectibles:
            let frame = rootView.viewModel.collectiblesFrame
            return frame.isEmpty ? nil : frame
        case let .card(key):
            guard rootView.viewModel.pocketCards.contains(where: { $0.key == key }) else { return nil }

            return rootView.viewModel.pocketCardFrames[key]
        }
    }

    var openItem: PocketItem? {
        rootView.viewModel.openItem
    }

    func setOpenItem(_ item: PocketItem?) {
        let previous = rootView.viewModel.openItem
        rootView.viewModel.openItem = item

        if item == .collectibles {
            liftCollectiblesPeek()
        } else if previous == .collectibles {
            landCollectiblesPeek()
        }
    }

    func moveOpenItem(by offset: CGSize, clearingPocket: Bool, animated: Bool) {
        let viewModel = rootView.viewModel
        withTransaction(Transaction(animation: animated ? PocketFlightTiming.animation : nil)) {
            viewModel.openPocketCardOffset = offset
            viewModel.isPocketCleared = clearingPocket
        }
    }

    private func openCollectibles() {
        guard rootView.viewModel.collectiblesPeek == .resting else { return }

        withAnimation(.linear(duration: CollectiblesBookTiming.colourDuration)) {
            rootView.viewModel.collectiblesPeek = .colouring
        }

        Task { [weak self] in
            try? await Task.sleep(for: .seconds(CollectiblesBookTiming.colourDuration))
            self?.presenter.showCollectibles()
        }
    }
}

private extension WalletMainViewController {
    func liftCollectiblesPeek() {
        withTransaction(Transaction(animation: nil)) {
            rootView.viewModel.collectiblesPeek = .lifted
        }
    }

    func landCollectiblesPeek() {
        guard rootView.viewModel.collectiblesPeek == .lifted else { return }

        withTransaction(Transaction(animation: nil)) {
            rootView.viewModel.collectiblesPeek = .colouring
        }
        DispatchQueue.main.async { [weak self] in
            withAnimation(.linear(duration: CollectiblesBookTiming.colourDuration)) {
                self?.rootView.viewModel.collectiblesPeek = .resting
            }
        }
    }
}
