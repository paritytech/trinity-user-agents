import Foundation
import Foundation_iOS
import ChainRegistry

@MainActor
final class BackupPresenter {
    // MARK: Properties

    weak var view: BackupViewProtocol?
    let wireframe: BackupWireframeProtocol
    let interactor: BackupInteractorInputProtocol
    private let chainAsset: ChainAsset
    private let balanceFormatterFactory: AssetBalanceFormatterFactoryProtocol
    private let logger: LoggerProtocol

    private var statusType: BackupViewModel.BackupStatusType?
    private var showsRecoveredBalance = false
    private var isRecoveryInProgress = false
    private var coinageAmounts: CoinageAmounts?

    // MARK: Initial methods

    init(
        interactor: BackupInteractorInputProtocol,
        wireframe: BackupWireframeProtocol,
        chainAsset: ChainAsset,
        logger: LoggerProtocol,
        balanceFormatterFactory: AssetBalanceFormatterFactoryProtocol = AssetBalanceFormatterFactory()
    ) {
        self.interactor = interactor
        self.wireframe = wireframe
        self.chainAsset = chainAsset
        self.logger = logger
        self.balanceFormatterFactory = balanceFormatterFactory
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

    private func provideViewModel() {
        view?.updateViewModel(
            BackupViewModel(statusType: statusType, restoredBalance: restoredBalanceViewModel())
        )
    }

    private func restoredBalanceViewModel() -> BackupRestoredBalanceViewModel? {
        guard showsRecoveredBalance else {
            return nil
        }

        return BackupRestoredBalanceViewModel(
            balance: coinageAmounts.map(restoredBalance),
            isUpdating: isRecoveryInProgress
        )
    }

    private func restoredBalance(from amounts: CoinageAmounts) -> BackupRestoredBalanceViewModel.Balance {
        let viewModelFactory = PrimitiveBalanceViewModelFactory(
            targetAssetInfo: chainAsset.asset.digitalDollarFigureDisplayInfo,
            formatterFactory: balanceFormatterFactory
        )

        func formatted(_ amount: Decimal) -> String {
            viewModelFactory.balanceFromPrice(amount, priceData: nil).value(for: .current).amount
        }

        return BackupRestoredBalanceViewModel.Balance(
            total: formatted(amounts.total),
            availableNow: amounts.hasFundsNotReady ? formatted(amounts.availableNow) : nil,
            symbol: chainAsset.asset.digitalDollarDisplayInfo.symbol
        )
    }
}

// MARK: - BackupPresenterProtocol

extension BackupPresenter: BackupPresenterProtocol {
    func setup() {
        interactor.setup()
        interactor.checkBackupStatus()
    }

    func onRestoredBalanceUpdate() {
        interactor.triggerRecovery()
    }

    func onRestoredBalanceClose() {
        wireframe.showCancelBackupConfirmation(from: view) { [weak self] in
            self?.interactor.acknowledgeRecovery()
        }
    }

    func onRestoredBalanceWhyUpdate() {
        wireframe.showWhyBackupUpdate(from: view)
    }

    func selectSecretPhrase() {
        wireframe.showWarning(from: view) { [weak self] in
            self?.authorize()
        }
    }

    func selectCloudBackup() {
        switch statusType {
        case .notFound:
            interactor.backupMnemonic()
        case .cloudIsOff:
            wireframe.showEnableCloud(from: view)
        case .created,
             nil:
            break
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
        statusType = status
        provideViewModel()
    }

    func didReceiveBackupComplete(with result: Result<Void, any Error>) {
        switch result {
        case .success:
            interactor.checkBackupStatus()
        case let .failure(error):
            showErrorAlert(with: error.localizedDescription)
        }
    }

    func didReceive(showsRecoveredBalance: Bool) {
        self.showsRecoveredBalance = showsRecoveredBalance
        provideViewModel()
    }

    func didReceive(isRecoveryInProgress: Bool) {
        self.isRecoveryInProgress = isRecoveryInProgress
        provideViewModel()
    }

    func didReceive(coinageAmounts: CoinageAmounts) {
        self.coinageAmounts = coinageAmounts
        provideViewModel()
    }
}
