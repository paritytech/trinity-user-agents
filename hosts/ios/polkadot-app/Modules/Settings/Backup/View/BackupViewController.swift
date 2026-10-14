import UIKit
import PolkadotUI
import FoundationExt

final class BackupViewController: UIViewController, ViewHolder {
    // MARK: Properties

    typealias RootViewType = BackupViewLayout

    let presenter: BackupPresenterProtocol

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
        presenter.setup()
        rootView.buttonsDelegate = self
    }

    // MARK: Private methods

    private func configureController() {
        title = String(localized: .backupMainTitle)
        view.backgroundColor = .bgSurfaceMain
    }
}

// MARK: - BackupViewProtocol

extension BackupViewController: BackupViewProtocol {
    func updateViewModel(_ viewModel: BackupViewModel) {
        rootView.bind(model: viewModel)
    }
}

// MARK: - BackupButtonsViewDelegate

extension BackupViewController: BackupButtonsViewDelegate {
    func didTapButton(_ type: BackupButtonsView.ButtonType) {
        presenter.handleButtonTap(type: type)
    }
}
