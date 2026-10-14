import UIKit
import PolkadotUI
import FoundationExt

final class SecretPhraseMnemonicViewController: UIViewController, ViewHolder {
    // MARK: Properties

    typealias RootViewType = SecretPhraseMnemonicViewLayout

    let presenter: SecretPhraseMnemonicPresenterProtocol

    // MARK: Initial methods

    init(presenter: SecretPhraseMnemonicPresenterProtocol) {
        self.presenter = presenter
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    // MARK: Life cycle

    override func loadView() {
        view = SecretPhraseMnemonicViewLayout()
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        observeApplicationState()
        configureController()
        presenter.setup()
    }

    // MARK: Private methods

    private func observeApplicationState() {
        let notificationCenter = NotificationCenter.default
        notificationCenter.addObserver(
            self,
            selector: #selector(applicationWillResignActiveNotification),
            name: UIApplication.willResignActiveNotification,
            object: nil
        )
    }

    private func configureController() {
        rootView.titleLabel.text = String(localized: .secretRecoveryTitle)
        rootView.subtitleLabel.text = String(localized: .secretRecoverySubtitle)

        rootView.mnemonicView.titleLabel.text = String(localized: .secretRecoveryViewTitle)
        rootView.mnemonicView.subtitleLabel.text = String(localized: .secretRecoveryViewSubtitle)

        rootView.onViewMnemonic = { [weak self] in
            self?.onViewMnemonic()
        }

        view.backgroundColor = .bgSurfaceMain
        rootView.delegate = self
    }

    // MARK: Notification

    @objc
    private func applicationWillResignActiveNotification() {
        rootView.hideData()
    }

    private func onViewMnemonic() {
        switch rootView.mnemonicView.currentType {
        case .hidden:
            presenter.onShowMnemonic()
        case .shown:
            presenter.onHideMnemonic()
        }
    }
}

// MARK: - SecretPhraseMnemonicViewProtocol

extension SecretPhraseMnemonicViewController: SecretPhraseMnemonicViewProtocol {
    func showMnemonic(_ show: Bool) {
        rootView.mnemonicView.currentType = show ? .shown : .hidden
    }

    func updateViewModel(_ viewModel: SecretPhraseMnemonicViewModel) {
        rootView.bind(viewModel: viewModel)
    }
}

// MARK: - SecretPhraseMnemonicViewDelegate

extension SecretPhraseMnemonicViewController: SecretPhraseMnemonicViewDelegate {
    func didTapCopyMnemonic(_ mnemonic: String) {
        presenter.copyDataInBuffer(mnemonic)
    }
}
