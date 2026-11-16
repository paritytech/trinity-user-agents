import UIKit
import SwiftUI

final class BackupViewController: UIHostingController<BackupViewLayout> {
    let presenter: BackupPresenterProtocol

    init(presenter: BackupPresenterProtocol) {
        self.presenter = presenter
        super.init(
            rootView: BackupViewLayout(
                viewModel: BackupViewModel(statusType: nil, restoredBalance: nil),
                presenter: presenter
            )
        )
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .bgSurfaceMain
        title = String(localized: .backupMainTitle)
        presenter.setup()
    }
}

// MARK: - BackupViewProtocol

extension BackupViewController: BackupViewProtocol {
    func updateViewModel(_ viewModel: BackupViewModel) {
        rootView = BackupViewLayout(viewModel: viewModel, presenter: presenter)
    }
}
