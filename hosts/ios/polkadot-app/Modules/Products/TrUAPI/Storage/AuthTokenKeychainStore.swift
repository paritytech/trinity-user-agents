import Foundation
import Security
import TrUAPIHost

/// Keychain operations the auth token store needs: items under an opaque key, each tagged with the
/// product it belongs to so one product's items can be counted and dropped.
protocol AuthTokenKeychain: Sendable {
    func set(_ data: Data, key: String, productId: String) throws
    func data(key: String) throws -> Data?
    func delete(key: String) throws
    func count(productId: String) throws -> Int
    func deleteAll(productId: String?) throws
}

/// ``AuthTokenHostBridge`` over the Keychain: one item per token, under the opaque key the core
/// derives from the signed-in account, the product and its service, so no lookup here can reach
/// another account's or product's token. The core clears tokens on sign-out and when a product's
/// state is cleared.
final class AuthTokenKeychainStore: AuthTokenHostBridge, @unchecked Sendable {
    private let keychain: AuthTokenKeychain

    init(keychain: AuthTokenKeychain = SecAuthTokenKeychain()) {
        self.keychain = keychain
    }

    func storeAuthToken(productId: String, key: String, token: AuthToken) async throws {
        try keychain.set(JSONEncoder().encode(StoredAuthToken(token)), key: key, productId: productId)
    }

    /// An item that no longer decodes is dropped and reads as absent, so the product signs in again
    /// instead of failing on every read.
    func authToken(productId _: String, key: String) async throws -> AuthToken? {
        guard let data = try keychain.data(key: key) else { return nil }
        guard let stored = try? JSONDecoder().decode(StoredAuthToken.self, from: data) else {
            try keychain.delete(key: key)
            return nil
        }
        return stored.authToken
    }

    func removeAuthToken(productId _: String, key: String) async throws {
        try keychain.delete(key: key)
    }

    func authTokenCount(productId: String) async throws -> UInt32 {
        try UInt32(clamping: keychain.count(productId: productId))
    }

    func clearAuthTokens(productId: String?) async throws {
        try keychain.deleteAll(productId: productId)
    }
}

/// The Keychain form of an ``AuthToken``.
private struct StoredAuthToken: Codable {
    let token: String
    let refreshToken: String?
    let expiresAt: UInt64?

    init(_ authToken: AuthToken) {
        token = authToken.token
        refreshToken = authToken.refreshToken
        expiresAt = authToken.expiresAt
    }

    var authToken: AuthToken {
        AuthToken(token: token, refreshToken: refreshToken, expiresAt: expiresAt)
    }
}

/// Generic-password items in this app's Keychain. Readable once the device has been unlocked after
/// a restart, so a provider worker resuming in the background can read its token, and never
/// migrated to another device.
struct SecAuthTokenKeychain: AuthTokenKeychain {
    static let service = "io.parity.truapi.auth-token"

    func set(_ data: Data, key: String, productId: String) throws {
        let attributes: [String: Any] = [kSecValueData as String: data, kSecAttrLabel as String: productId]
        let status = SecItemUpdate(Self.query(key: key) as CFDictionary, attributes as CFDictionary)
        guard status == errSecItemNotFound else {
            return try Self.check(status, "update")
        }
        let item = Self.query(key: key)
            .merging(attributes) { $1 }
            .merging([kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly]) { $1 }
        try Self.check(SecItemAdd(item as CFDictionary, nil), "add")
    }

    func data(key: String) throws -> Data? {
        var query = Self.query(key: key)
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var result: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        if status == errSecItemNotFound { return nil }
        try Self.check(status, "read")
        return result as? Data
    }

    func delete(key: String) throws {
        let status = SecItemDelete(Self.query(key: key) as CFDictionary)
        guard status != errSecItemNotFound else { return }
        try Self.check(status, "delete")
    }

    func count(productId: String) throws -> Int {
        var query = Self.query(productId: productId)
        query[kSecReturnAttributes as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitAll
        var result: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        if status == errSecItemNotFound { return 0 }
        try Self.check(status, "count")
        return (result as? [[String: Any]])?.count ?? 0
    }

    func deleteAll(productId: String?) throws {
        let status = SecItemDelete(Self.query(productId: productId) as CFDictionary)
        guard status != errSecItemNotFound else { return }
        try Self.check(status, "clear")
    }

    private static func query(key: String) -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: key
        ]
    }

    private static func query(productId: String?) -> [String: Any] {
        var query: [String: Any] = [kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: service]
        if let productId {
            query[kSecAttrLabel as String] = productId
        }
        return query
    }

    private static func check(_ status: OSStatus, _ operation: String) throws {
        guard status == errSecSuccess else {
            throw AuthTokenKeychainError(operation: operation, status: status)
        }
    }
}

/// A Keychain call that failed, named by what it tried and the status it got. Carries no token.
struct AuthTokenKeychainError: LocalizedError {
    let operation: String
    let status: OSStatus

    var errorDescription: String? {
        "auth token Keychain \(operation) failed with status \(status)"
    }
}
