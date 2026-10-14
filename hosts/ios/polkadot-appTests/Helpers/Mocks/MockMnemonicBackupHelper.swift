@testable import polkadot_app
import Foundation
import NovaCrypto

final class MockMnemonicBackupHelper {
    private var mnemonic: IRMnemonicProtocol?
    private let mutex = NSLock()
    var isAvailable: Bool

    init(mnemonic: IRMnemonicProtocol? = nil, isAvailable: Bool = true) {
        self.mnemonic = mnemonic
        self.isAvailable = isAvailable
    }
}

enum MockMnemonicBackupHelperError: Error {
    case mnemonicUnvailable
}

extension MockMnemonicBackupHelper: MnemonicBackupHelperProtocol {
    var didChangeAvailabilityNotification: Notification.Name {
        Notification.Name(rawValue: "DidChangeAvailability")
    }

    func checkForBackup() throws -> Bool {
        true
    }

    func saveMnemonic(_ mnemonic: IRMnemonicProtocol) throws {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        self.mnemonic = mnemonic
    }

    func fetchMnemonic() throws -> IRMnemonicProtocol {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        guard let mnemonic else {
            throw MockMnemonicBackupHelperError.mnemonicUnvailable
        }

        return mnemonic
    }

    func deleteMnemonic() throws {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        mnemonic = nil
    }
}
