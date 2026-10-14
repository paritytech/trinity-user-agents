import Foundation
import Keystore_iOS
import SubstrateSdk
import KeyDerivation

protocol BackendAuthStoring {
    func fetchAuthWallet() throws -> WalletManaging
}

final class BackendAuthStore: BackendAuthStoring {
    private let keychain: KeystoreProtocol
    private let sessionIdStore: BackendSessionIdStoring

    init(
        keychain: KeystoreProtocol = Keychain(),
        sessionIdStore: BackendSessionIdStoring = BackendSessionIdStore()
    ) {
        self.keychain = keychain
        self.sessionIdStore = sessionIdStore
    }

    func fetchAuthWallet() throws -> WalletManaging {
        let seedBytes = try fetchOrCreateSeedBytes()
        return try DynamicDerivedWallet(seedBytes: seedBytes)
    }
}

private extension BackendAuthStore {
    func fetchOrCreateSeedBytes() throws -> Data {
        let sessionId = sessionIdStore.getOrCreateSessionId()
        let tag = KeystoreTag.backendClientTag(for: sessionId)

        do {
            return try keychain.fetchKey(for: tag)
        } catch KeystoreError.noKeyFound {
            let seedBytes = try Data.randomOrError(of: 32)
            try keychain.saveKey(seedBytes, with: tag)
            return seedBytes
        }
    }
}
