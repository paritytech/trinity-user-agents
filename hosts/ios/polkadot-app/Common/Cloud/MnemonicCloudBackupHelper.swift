import Foundation
import NovaCrypto

protocol MnemonicBackupHelperProtocol {
    var isAvailable: Bool { get }
    var didChangeAvailabilityNotification: Notification.Name { get }
    func checkForBackup() throws -> Bool
    func saveMnemonic(_ mnemonic: IRMnemonicProtocol) throws
    func fetchMnemonic() throws -> IRMnemonicProtocol
    func deleteMnemonic() throws
}

final class MnemonicBackupHelper: MnemonicBackupHelperProtocol {
    private let mnemonicGenerator: IRMnemonicCreatorProtocol
    private let cloudKeychain: SynchronizableKeychainProtocol
    private let logger: LoggerProtocol

    init(
        mnemonicGenerator: IRMnemonicCreatorProtocol = IRMnemonicCreator(),
        cloudKeychain: SynchronizableKeychainProtocol = SynchronizableKeychain(),
        logger: LoggerProtocol = Logger.shared
    ) {
        self.mnemonicGenerator = mnemonicGenerator
        self.cloudKeychain = cloudKeychain
        self.logger = logger
    }

    var isAvailable: Bool {
        cloudKeychain.isAvailable
    }

    var didChangeAvailabilityNotification: Notification.Name {
        cloudKeychain.didChangeAvailabilityNotification
    }

    func checkForBackup() throws -> Bool {
        try cloudKeychain.checkKey(for: SynchronizableKeychainTag.walletEntropy)
    }

    func saveMnemonic(_ mnemonic: any IRMnemonicProtocol) throws {
        try cloudKeychain.addKey(mnemonic.entropy(), with: SynchronizableKeychainTag.walletEntropy)
    }

    func fetchMnemonic() throws -> any IRMnemonicProtocol {
        let entropy = try cloudKeychain.fetchKey(for: SynchronizableKeychainTag.walletEntropy)
        return try mnemonicGenerator.mnemonic(fromEntropy: entropy)
    }

    func deleteMnemonic() throws {
        try cloudKeychain.deleteKey(for: SynchronizableKeychainTag.walletEntropy)
    }
}
