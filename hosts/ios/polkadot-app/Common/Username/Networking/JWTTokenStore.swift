import Foundation
import KeyDerivation
import Keystore_iOS

protocol JWTTokenStoring {
    func saveToken(_ token: String) throws
    func fetchToken() -> String?
    func deleteToken()

    func saveRefreshToken(_ token: String) throws
    func fetchRefreshToken() -> String?
    func deleteRefreshToken()

    func deleteAll()
}

final class JWTTokenStore: JWTTokenStoring {
    private let keychain: KeystoreProtocol
    private let sessionIdStore: BackendSessionIdStoring

    init(
        keychain: KeystoreProtocol = Keychain(),
        sessionIdStore: BackendSessionIdStoring = BackendSessionIdStore()
    ) {
        self.keychain = keychain
        self.sessionIdStore = sessionIdStore
    }

    func saveToken(_ token: String) throws {
        guard let data = token.data(using: .utf8) else {
            assertionFailure()
            return
        }

        try keychain.saveKey(data, with: tokenTag)
    }

    func fetchToken() -> String? {
        guard let data = try? keychain.fetchKey(for: tokenTag) else {
            return nil
        }

        return String(data: data, encoding: .utf8)
    }

    func deleteToken() {
        try? keychain.deleteKey(for: tokenTag)
    }

    func saveRefreshToken(_ token: String) throws {
        guard let data = token.data(using: .utf8) else {
            assertionFailure()
            return
        }

        try keychain.saveKey(data, with: refreshTokenTag)
    }

    func fetchRefreshToken() -> String? {
        guard let data = try? keychain.fetchKey(for: refreshTokenTag) else {
            return nil
        }

        return String(data: data, encoding: .utf8)
    }

    func deleteRefreshToken() {
        try? keychain.deleteKey(for: refreshTokenTag)
    }

    func deleteAll() {
        deleteToken()
        deleteRefreshToken()
    }
}

private extension JWTTokenStore {
    var tokenTag: String {
        KeystoreTag.jwtTokenTag(for: sessionIdStore.getOrCreateSessionId())
    }

    var refreshTokenTag: String {
        KeystoreTag.jwtRefreshTokenTag(for: sessionIdStore.getOrCreateSessionId())
    }
}
