import Foundation
import NovaCrypto
import Operation_iOS
import Keystore_iOS
import KeyDerivation

protocol WalletSetupManaging {
    func createWallets(with metadata: AccountCreateMetadata?) throws
    func restoreWallets() throws
}

final class WalletSetupManager {
    let mnemonicGenerator: IRMnemonicCreatorProtocol
    let entropyManager: RootEntropyManaging
    let mnemonicBackupHelper: MnemonicBackupHelperProtocol
    let logger: LoggerProtocol

    init(
        mnemonicGenerator: IRMnemonicCreatorProtocol,
        mnemonicBackupHelper: MnemonicBackupHelperProtocol,
        entropyManager: RootEntropyManaging,
        logger: LoggerProtocol
    ) {
        self.mnemonicGenerator = mnemonicGenerator
        self.mnemonicBackupHelper = mnemonicBackupHelper
        self.entropyManager = entropyManager
        self.logger = logger
    }
}

private extension WalletSetupManager {
    private func createWallets(
        fromMnemonic mnemonic: IRMnemonicProtocol,
        shouldBackup: Bool
    ) throws {
        try entropyManager.createRootEntropy(mnemonic.entropy())

        if shouldBackup {
            try mnemonicBackupHelper.saveMnemonic(mnemonic)
        }
    }
}

extension WalletSetupManager: WalletSetupManaging {
    func createWallets(with metadata: AccountCreateMetadata?) throws {
        let mnemonic: IRMnemonicProtocol =
            if let someMnemonic = metadata?.mnemonic {
                someMnemonic
            } else {
                try mnemonicGenerator.randomMnemonic(.entropy128)
            }
        let shouldBackup = metadata == nil // do not create backup if account create data provided
        return try createWallets(fromMnemonic: mnemonic, shouldBackup: shouldBackup)
    }

    func restoreWallets() throws {
        let mnemonic = try mnemonicBackupHelper.fetchMnemonic()
        return try createWallets(fromMnemonic: mnemonic, shouldBackup: false)
    }
}
