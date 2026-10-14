import Foundation
import Keystore_iOS
import KeyDerivation

final class MockInstallationKeyIdStore: InstallationKeyIdStoring, @unchecked Sendable {
    private var installationKeyId: String?
    private let mutex = NSLock()

    init(installationKeyId: String? = nil) {
        self.installationKeyId = installationKeyId
    }

    func saveInstallationKeyId(_ installationKeyId: String) {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        self.installationKeyId = installationKeyId
    }

    func getInstallationKeyId() -> String? {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        return installationKeyId
    }
}
