import UIKitExt

protocol BackupViewProtocol: ControllerBackedProtocol {
    func updateViewModel(_ viewModel: BackupViewModel)
}

@MainActor
protocol BackupPresenterProtocol: AnyObject {
    func setup()
    func handleButtonTap(type: BackupButtonsView.ButtonType)
}

protocol BackupInteractorInputProtocol: AnyObject {
    func checkBackupStatus()
    func backupMnemonic()
}

@MainActor
protocol BackupInteractorOutputProtocol: AnyObject {
    func didReceiveBackupStatus(_ status: BackupViewModel.BackupStatusType)
    func didReceiveBackupComplete(with result: Result<Void, Error>)
}

@MainActor
protocol BackupWireframeProtocol: AlertPresentable, AuthorizationPresentable, ApplicationSettingsPresentable {
    func showWarning(
        from view: (any BackupViewProtocol)?,
        action: @escaping () -> Void
    )
    func showEnableCloud(from view: BackupViewProtocol?)
    func openSecretRecoveryPhase(from view: BackupViewProtocol?)
}
