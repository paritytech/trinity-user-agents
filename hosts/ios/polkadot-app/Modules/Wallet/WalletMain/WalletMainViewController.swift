import SwiftUI
import ExternalAccessibility
import PolkadotUI

final class WalletMainViewController: UIHostingController<WalletView>, RootScreen, NavigationDependable,
    NavigationBarHiding {
    let presenter: WalletMainPresenterProtocol
    weak var navigationControlling: NavigationControlling?

    var hidesNavigationBar: Bool {
        rootView.viewModel.expandedSection == .none
    }

    private var assetDetailsScene: AssetDetailsScene

    private let titleLabel: PolkadotUI.Label = .create { view in
        view.typography = .headlineSmall
        view.textColor = .fgPrimary
    }

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
        setMainTitle()

        assetDetailsScene.attachNavigationHost(self)
        setupHandlers()
        presenter.setup()
    }

    private func updateNavigationBarVisibility(animated: Bool) {
        navigationControlling?.setNavigationBarHidden(hidesNavigationBar, animated: animated)
    }

    private func setupHandlers() {
        rootView.viewModel.assetDetailsViewModel = assetDetailsScene.viewModel

        rootView.viewModel.onBalance = { [weak self] in
            self?.showAssetDetailsOverlay()
        }

        rootView.viewModel.onCollapse = { [weak self] in
            self?.collapseExpandedSection()
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

    private func resetRightButton() {
        navigationItem.setRightBarButton(nil, animated: true)
    }

    private func resetOverlayCloseButton() {
        navigationItem.setLeftBarButton(nil, animated: true)
    }

    private func setOverlayCloseButton() {
        let item = UIBarButtonItem(
            image: .buttonClose.withRenderingMode(.alwaysTemplate),
            style: .plain,
            target: self,
            action: #selector(collapseExpandedSection)
        )
        item.accessibilityId(AccessibilityID.Wallet.detailCloseButton)
        navigationItem.setLeftBarButton(item, animated: true)
    }
}

extension WalletMainViewController: WalletMainViewProtocol {
    func didReceive(isCollectiblesAvailable: Bool) {
        rootView.viewModel.isCollectiblesAvailable = isCollectiblesAvailable
    }

    func didReceive(pocketCards: [PocketCardViewModel]) {
        rootView.viewModel.pocketCards = pocketCards
    }

    /// Nil once the card is gone or the Wallet is not what closing the card
    /// returns to.
    func frame(of key: PocketCardKey) -> CGRect? {
        guard
            UIApplication.shared.mainTabBarController?.container.selectedController === navigationController,
            navigationController?.viewControllers.suffix(2).first === self,
            rootView.viewModel.expandedSection == .none,
            rootView.viewModel.pocketCards.contains(where: { $0.key == key })
        else { return nil }

        return rootView.viewModel.pocketCardFrames[key]
    }

    var openCard: PocketCardKey? {
        rootView.viewModel.openPocketCard
    }

    func setOpenCard(_ key: PocketCardKey?) {
        rootView.viewModel.openPocketCard = key
    }

    func moveOpenCard(by offset: CGSize, clearingPocket: Bool, animated: Bool) {
        let viewModel = rootView.viewModel
        withTransaction(Transaction(animation: animated ? PocketCardFlightTiming.animation : nil)) {
            viewModel.openPocketCardOffset = offset
            viewModel.isPocketCleared = clearingPocket
        }
    }

    @objc
    func collapseExpandedSection() {
        rootView.viewModel.expandedSection = .none
        resetOverlayCloseButton()
        resetRightButton()
        setMainTitle()
    }

    private func showAssetDetailsOverlay() {
        rootView.viewModel.expandedSection = .assetDetails
        setOverlayCloseButton()
        resetRightButton()
        setTitleCenter(String(localized: .walletMainPurse))
    }

    private func openCollectibles() {
        guard
            rootView.viewModel.expandedSection == .none,
            rootView.viewModel.collectiblesPeek == .resting
        else { return }

        withAnimation(.linear(duration: CollectiblesBookTiming.colourDuration)) {
            rootView.viewModel.collectiblesPeek = .colouring
        }

        Task { [weak self] in
            try? await Task.sleep(for: .seconds(CollectiblesBookTiming.colourDuration))
            self?.presenter.showCollectibles()
        }
    }

    func setMainTitle() {
        navigationItem.style = .browser
        navigationItem.title = nil
        navigationItem.titleView = nil
        updateNavigationBarVisibility(animated: true)
    }

    func setTitleCenter(_ title: String?) {
        navigationItem.style = .navigator
        updateNavigationBarVisibility(animated: true)
        guard #available(iOS 26, *) else {
            navigationItem.title = title
            return
        }
        titleLabel.typography = .titleLarge
        titleLabel.attributedText = title.map {
            CashLogo.styled($0, symbol: PaymentAssetBranding.shared.current.symbol, font: UIFont.app(.titleLarge))
        }
        titleLabel.accessibilityLabel = title
        titleLabel.sizeToFit()
        navigationItem.titleView = titleLabel
    }
}

extension WalletMainViewController: CollectiblesOrigin {
    var collectiblesBookFrame: CGRect? {
        let frame = rootView.viewModel.collectiblesFrame
        return frame.isEmpty ? nil : frame
    }

    func setCollectiblesBookLifted() {
        withTransaction(Transaction(animation: nil)) {
            rootView.viewModel.collectiblesPeek = .lifted
        }
    }

    func setCollectiblesBookLanded() {
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
