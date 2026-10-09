import SwiftUI
import ExternalAccessibility
import PolkadotUI

final class WalletMainViewController: UIHostingController<WalletView>, RootScreen {
    let presenter: WalletMainPresenterProtocol

    private var assetDetailsScene: AssetDetailsScene
    private var identityDetailsScene: IdentityDetailsScene

    private let titleLabel: PolkadotUI.Label = .create { view in
        view.typography = .headlineSmall
        view.textColor = .fgPrimary
    }

    private let statusTitleView = NetworkStatusTitleView()

    init(
        presenter: WalletMainPresenterProtocol,
        assetDetailsScene: AssetDetailsScene,
        identityDetailsScene: IdentityDetailsScene
    ) {
        self.presenter = presenter
        self.assetDetailsScene = assetDetailsScene
        self.identityDetailsScene = identityDetailsScene
        super.init(rootView: WalletView())
    }

    @available(*, unavailable)
    @MainActor @preconcurrency dynamic required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .bgSurfaceMain
        setMainTitle()

        assetDetailsScene.attachNavigationHost(self)
        identityDetailsScene.attachNavigationHost(self)
        setupHandlers()
        presenter.setup()
    }

    private func setupHandlers() {
        rootView.viewModel.identityDetailsViewModel = identityDetailsScene.viewModel
        rootView.viewModel.assetDetailsViewModel = assetDetailsScene.viewModel
        rootView.viewModel.onUsername = { [weak self] in
            self?.showIdentityDetailsOverlay()
        }

        rootView.viewModel.onBalance = { [weak self] in
            self?.showAssetDetailsOverlay()
        }

        rootView.viewModel.onCollapse = { [weak self] in
            self?.collapseExpandedSection()
        }

        rootView.viewModel.onCollectibles = { [weak self] in
            self?.showCollectiblesOverlay()
        }

        rootView.viewModel.onViewCollectibles = { [weak presenter] in
            presenter?.showCollectibles()
        }
    }

    private func setShareButton() {
        let item = UIBarButtonItem(
            image: .iconShareWallet,
            style: .plain,
            target: self,
            action: #selector(didTapShare)
        )
        navigationItem.setRightBarButton(item, animated: true)
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

    @objc
    func didTapShare() {
        identityDetailsScene.share()
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
        setTitleCenter(String(localized: .walletMainBalanceCard(PaymentAssetBranding.shared.current.symbol)))
    }

    private func showIdentityDetailsOverlay() {
        rootView.viewModel.expandedSection = .identityDetails
        setOverlayCloseButton()
        setShareButton()
        setTitleCenter(String(localized: .walletMainIdCard))
    }

    private func showCollectiblesOverlay() {
        rootView.viewModel.expandedSection = .collectiblesDetails
        setOverlayCloseButton()
        resetRightButton()
        setTitleCenter(nil)
    }

    func setMainTitle() {
        navigationItem.style = .browser
        if #unavailable(iOS 26.0) {
            navigationItem.title = nil
        }
        setTitleView(statusTitleView)
    }

    func setTitleCenter(_ title: String?) {
        navigationItem.style = .navigator
        guard #available(iOS 26, *) else {
            navigationItem.title = title
            return
        }
        titleLabel.text = title
        titleLabel.typography = .titleLarge
        titleLabel.sizeToFit()
        navigationItem.titleView = titleLabel
    }

    func didReceive(titleViewModel: NetworkStatusTitleView.ViewModel) {
        statusTitleView.bind(viewModel: titleViewModel)
    }
}
