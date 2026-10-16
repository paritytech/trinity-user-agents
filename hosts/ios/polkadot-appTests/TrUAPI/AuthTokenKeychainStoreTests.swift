import Foundation
import Testing
import TrUAPIHost

@testable import polkadot_app

/// An in-memory ``AuthTokenKeychain`` keyed the way the Keychain store keys its items.
final class InMemoryAuthTokenKeychain: AuthTokenKeychain, @unchecked Sendable {
    private let lock = NSLock()
    private var items: [String: (productId: String, account: String, data: Data)] = [:]

    func set(_ data: Data, key: String, productId: String, account: String) throws {
        lock.withLock { items[key] = (productId, account, data) }
    }

    func data(key: String) throws -> Data? {
        lock.withLock { items[key]?.data }
    }

    func delete(key: String) throws {
        lock.withLock { _ = items.removeValue(forKey: key) }
    }

    func count(productId: String, account: String) throws -> Int {
        lock.withLock { items.values.filter { $0.productId == productId && $0.account == account }.count }
    }

    func deleteAll(productId: String?) throws {
        lock.withLock {
            items = productId.map { productId in items.filter { $0.value.productId != productId } } ?? [:]
        }
    }
}

struct AuthTokenKeychainStoreTests {
    private let keychain = InMemoryAuthTokenKeychain()
    private let token = AuthToken(token: "jwt", refreshToken: "refresh", expiresAt: 1_900_000_000_000)

    @Test("a kept token reads back under its key, and only its product's tokens are counted")
    func keptTokenReadsBack() async throws {
        let store = AuthTokenKeychainStore(keychain: keychain)

        try await store.storeAuthToken(productId: "wallet.dot", account: "alice", key: "k1", token: token)
        try await store.storeAuthToken(productId: "shop.dot", account: "alice", key: "k2", token: token)

        #expect(try await store.authToken(productId: "wallet.dot", key: "k1") == token)
        #expect(try await store.authToken(productId: "wallet.dot", key: "missing") == nil)
        #expect(try await store.authTokenCount(productId: "wallet.dot", account: "alice") == 1)
    }

    @Test("the count covers one account, so another account's tokens never fill the cap")
    func countIsPerAccount() async throws {
        let store = AuthTokenKeychainStore(keychain: keychain)
        try await store.storeAuthToken(productId: "wallet.dot", account: "alice", key: "k1", token: token)
        try await store.storeAuthToken(productId: "wallet.dot", account: "bob", key: "k2", token: token)
        try await store.storeAuthToken(productId: "wallet.dot", account: "bob", key: "k3", token: token)

        #expect(try await store.authTokenCount(productId: "wallet.dot", account: "alice") == 1)
        #expect(try await store.authTokenCount(productId: "wallet.dot", account: "bob") == 2)
    }

    @Test("clearing one product keeps the others, and clearing everything leaves none")
    func clearingDropsOneProductOrAll() async throws {
        let store = AuthTokenKeychainStore(keychain: keychain)
        try await store.storeAuthToken(productId: "wallet.dot", account: "alice", key: "k1", token: token)
        try await store.storeAuthToken(productId: "shop.dot", account: "alice", key: "k2", token: token)

        try await store.clearAuthTokens(productId: "wallet.dot")
        let afterProduct = try await (
            store.authToken(productId: "wallet.dot", key: "k1"),
            store.authToken(productId: "shop.dot", key: "k2")
        )
        try await store.clearAuthTokens(productId: nil)

        #expect(afterProduct.0 == nil)
        #expect(afterProduct.1 == token)
        #expect(try await store.authToken(productId: "shop.dot", key: "k2") == nil)
    }

    @Test("an item that no longer decodes reads as absent and is dropped")
    func undecodableItemReadsAsAbsent() async throws {
        let store = AuthTokenKeychainStore(keychain: keychain)
        try keychain.set(Data("not a token".utf8), key: "k1", productId: "wallet.dot", account: "alice")

        #expect(try await store.authToken(productId: "wallet.dot", key: "k1") == nil)
        #expect(try keychain.count(productId: "wallet.dot", account: "alice") == 0)
    }
}
