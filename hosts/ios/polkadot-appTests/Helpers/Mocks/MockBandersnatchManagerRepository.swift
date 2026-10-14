import Foundation
@testable import polkadot_app
import KeyDerivation
import Keystore_iOS
import SubstrateSdk

/// Derives real personhood ring-VRF managers over an in-memory entropy manager for a fixed TLD.
final class MockBandersnatchManagerRepository: BandersnatchManagerRepositoryProtocol {
    let tld: String
    let entropyManager: RootEntropyManaging

    init(tld: String = "dot", entropyManager: RootEntropyManaging) {
        self.tld = tld
        self.entropyManager = entropyManager
    }

    convenience init(tld: String = "dot") throws {
        let manager = RootEntropyManager(
            keychain: InMemoryKeychain(),
            installationKeyIdStore: MockInstallationKeyIdStore()
        )
        try manager.createRootEntropy(Data.randomOrError(of: 32))
        self.init(tld: tld, entropyManager: manager)
    }

    func fullPerson() throws -> BandersnatchKeyManager {
        BandersnatchKeyManager.fullPerson(for: tld, entropyManager: entropyManager)
    }

    func litePerson() throws -> BandersnatchKeyManager {
        BandersnatchKeyManager.litePerson(for: tld, entropyManager: entropyManager)
    }
}
