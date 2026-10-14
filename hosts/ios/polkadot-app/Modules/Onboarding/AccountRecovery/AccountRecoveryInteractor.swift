import UIKit
import NovaCrypto
import Operation_iOS

final class AccountRecoveryInteractor {
    weak var presenter: AccountRecoveryInteractorOutputProtocol?

    private let mnemonicGenerator: IRMnemonicCreatorProtocol
    private let walletSetupManager: WalletSetupManaging
    private let operationQueue: OperationQueue
    private let logger: LoggerProtocol

    init(
        mnemonicGenerator: IRMnemonicCreatorProtocol,
        walletSetupManager: WalletSetupManaging,
        operationQueue: OperationQueue,
        logger: LoggerProtocol
    ) {
        self.mnemonicGenerator = mnemonicGenerator
        self.walletSetupManager = walletSetupManager
        self.operationQueue = operationQueue
        self.logger = logger
    }
}

extension AccountRecoveryInteractor: AccountRecoveryInteractorInputProtocol {
    func proceed(withWords words: String) {
        guard let mnemonic = try? mnemonicGenerator.mnemonic(fromList: words) else {
            logger.debug("Invalid mnemonic format")
            MainActor.assumeIsolated {
                presenter?.didReceiveInvalidMnemonicFormat()
            }
            return
        }

        performCreateWallets(with: .init(mnemonic: mnemonic))
    }
}

private extension AccountRecoveryInteractor {
    func performCreateWallets(with metadata: AccountCreateMetadata) {
        MainActor.assumeIsolated {
            presenter?.authorizeUser { [weak self] isAuthorized in
                if isAuthorized {
                    self?.continueCreateWallets(with: metadata)
                }
            }
        }
    }

    func continueCreateWallets(with metadata: AccountCreateMetadata) {
        do {
            try walletSetupManager.createWallets(with: metadata)
            MainActor.assumeIsolated {
                presenter?.didRestoreWallets()
            }
        } catch {
            logger.error("Unexpected wallet create error: \(error)")
            MainActor.assumeIsolated {
                presenter?.didDecideBroken()
            }
        }
    }
}
