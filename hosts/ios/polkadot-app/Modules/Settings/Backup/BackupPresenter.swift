import Foundation

@MainActor
final class BackupPresenter {
    // MARK: Properties

    weak var view: BackupViewProtocol?
    let wireframe: BackupWireframeProtocol
    let interactor: BackupInteractorInputProtocol
    private let logger: LoggerProtocol

    // MARK: Initial methods

    init(
        interactor: BackupInteractorInputProtocol,
        wireframe: BackupWireframeProtocol,
        logger: LoggerProtocol
    ) {
        self.interactor = interactor
        self.wireframe = wireframe
        self.logger = logger
    }

    // MARK: Private methods

    private func showErrorAlert(with description: String? = nil) {
        wireframe.present(
            message: String(localized: .Common.error),
            title: description,
            closeAction: String(localized: .Common.close),
            from: view
        )
    }
}

// MARK: - BackupPresenterProtocol

extension BackupPresenter: BackupPresenterProtocol {
    func setup() {
        interactor.checkBackupStatus()
    }

    func handleButtonTap(type: BackupButtonsView.ButtonType) {
        switch type {
        case .backup:
            interactor.backupMnemonic()
        case .secretPhase:
            wireframe.showWarning(from: view) { [weak self] in
                self?.authorize()
            }
        case .settings:
            wireframe.showEnableCloud(from: view)
        }
    }
}

private extension BackupPresenter {
    func authorize() {
        wireframe.authorizeInPlace { [weak wireframe, weak view] authorized in
            guard authorized else {
                return
            }
            wireframe?.openSecretRecoveryPhase(from: view)
        }
    }
}

// MARK: - BackupInteractorOutputProtocol

extension BackupPresenter: BackupInteractorOutputProtocol {
    func didReceiveBackupStatus(_ status: BackupViewModel.BackupStatusType) {
        view?.updateViewModel(BackupViewModel(statusType: status))
    }

    func didReceiveBackupComplete(with result: Result<Void, any Error>) {
        switch result {
        case .success:
            interactor.checkBackupStatus()
        case let .failure(error):
            showErrorAlert(with: error.localizedDescription)
        }
    }
}
