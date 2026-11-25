import UIKitExt
import ChainRegistry

protocol BackupViewProtocol: ControllerBackedProtocol {
    func updateViewModel(_ viewModel: BackupViewModel)
}

@MainActor
protocol BackupPresenterProtocol: AnyObject {
    func setup()
    func selectSecretPhrase()
    func selectCloudBackup()
    func onRestoredBalanceUpdate()
    func onRestoredBalanceClose()
    func onRestoredBalanceWhyUpdate()
}

protocol BackupInteractorInputProtocol: AnyObject {
    func setup()
    func checkBackupStatus()
    func backupMnemonic()
    func triggerRecovery()
    func acknowledgeRecovery()
}

@MainActor
protocol BackupInteractorOutputProtocol: AnyObject {
    func didReceiveBackupStatus(_ status: BackupViewModel.BackupStatusType)
    func didReceiveBackupComplete(with result: Result<Void, Error>)
    func didReceive(showsRecoveredBalance: Bool)
    func didReceive(isRecoveryInProgress: Bool)
    func didReceive(coinageAmounts: CoinageAmounts)
}

@MainActor
protocol BackupWireframeProtocol: AlertPresentable,
    AuthorizationPresentable,
    ApplicationSettingsPresentable,
    BackupSyncPresentable {
    func showWarning(
        from view: (any BackupViewProtocol)?,
        action: @escaping () -> Void
    )
    func showEnableCloud(from view: BackupViewProtocol?)
    func openSecretRecoveryPhase(from view: BackupViewProtocol?)
}
