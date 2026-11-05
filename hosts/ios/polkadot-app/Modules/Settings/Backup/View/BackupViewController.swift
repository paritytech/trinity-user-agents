import UIKit
import SwiftUI
import SnapKit
import PolkadotUI
import FoundationExt

final class BackupViewController: UIViewController, ViewHolder {
    // MARK: Properties

    typealias RootViewType = BackupViewLayout

    let presenter: BackupPresenterProtocol

    private lazy var restoredBalanceHost = UIHostingController(rootView: makeRestoredBalanceCard(nil))

    // MARK: Initial methods

    init(presenter: BackupPresenterProtocol) {
        self.presenter = presenter
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    // MARK: Life cycle

    override func loadView() {
        view = BackupViewLayout()
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        configureController()
        configureRestoredBalanceHost()
        presenter.setup()
        rootView.buttonsDelegate = self
    }

    // MARK: Private methods

    private func configureController() {
        title = String(localized: .backupMainTitle)
        view.backgroundColor = .bgSurfaceMain
    }

    private func configureRestoredBalanceHost() {
        restoredBalanceHost.view.backgroundColor = .clear
        restoredBalanceHost.sizingOptions = [.intrinsicContentSize]
        addChild(restoredBalanceHost)
        rootView.restoredBalanceContainer.addSubview(restoredBalanceHost.view)
        restoredBalanceHost.view.snp.makeConstraints {
            $0.edges.equalToSuperview()
        }
        restoredBalanceHost.didMove(toParent: self)
    }

    private func makeRestoredBalanceCard(_ viewModel: BackupRestoredBalanceViewModel?) -> BackupRestoredBalanceCard {
        BackupRestoredBalanceCard(
            viewModel: viewModel ?? BackupRestoredBalanceViewModel(balance: nil, isUpdating: false),
            onUpdate: { [weak self] in self?.presenter.onRestoredBalanceUpdate() },
            onClose: { [weak self] in self?.presenter.onRestoredBalanceClose() },
            onWhyUpdate: { [weak self] in self?.presenter.onRestoredBalanceWhyUpdate() }
        )
    }
}

// MARK: - BackupViewProtocol

extension BackupViewController: BackupViewProtocol {
    func updateViewModel(_ viewModel: BackupViewModel) {
        rootView.bind(model: viewModel)
    }

    func updateRestoredBalance(_ viewModel: BackupRestoredBalanceViewModel?) {
        restoredBalanceHost.rootView = makeRestoredBalanceCard(viewModel)
        rootView.setRestoredBalanceVisible(viewModel != nil)
    }
}

// MARK: - BackupButtonsViewDelegate

extension BackupViewController: BackupButtonsViewDelegate {
    func didTapButton(_ type: BackupButtonsView.ButtonType) {
        presenter.handleButtonTap(type: type)
    }
}
