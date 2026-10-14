import Foundation
import Keystore_iOS

/// Identifies the resource type for key storage.
public enum ResourceKeyKind: String, CaseIterable {
    case statementStore = "statement-store"
    case bulletIn = "bulletin"
}

/// Stores and retrieves resource private keys per product, per resource kind.
public protocol ProductResourceKeyManaging {
    func storeResourceKey(
        _ key: Data,
        for productId: ProductId,
        kind: ResourceKeyKind
    ) throws

    func fetchResourceKey(
        for productId: ProductId,
        kind: ResourceKeyKind
    ) throws -> Data?

    func hasResourceKey(
        for productId: ProductId,
        kind: ResourceKeyKind
    ) throws -> Bool
}

// MARK: - Store ID

/// Provides a persistent store identifier to namespace keychain entries
/// and prevent collisions between different app installations.
public protocol ProductResourceStoreIdProviding {
    func getStoreId() -> String
}

public final class ProductResourceStoreIdStore: ProductResourceStoreIdProviding {
    static let storeIdKey = "io.polkadot.app.product.resource.store.id"

    private let userDefaults: UserDefaults

    public init(userDefaults: UserDefaults) {
        self.userDefaults = userDefaults
    }

    public func getStoreId() -> String {
        if let existing = userDefaults.string(forKey: Self.storeIdKey) {
            return existing
        }

        let newId = UUID().uuidString
        userDefaults.set(newId, forKey: Self.storeIdKey)
        userDefaults.synchronize()
        return newId
    }
}

// MARK: - Implementation

public final class ProductResourceKeyManager: ProductResourceKeyManaging {
    private let keychain: KeystoreProtocol
    private let storeIdProvider: ProductResourceStoreIdProviding

    public init(
        keychain: KeystoreProtocol,
        storeIdProvider: ProductResourceStoreIdProviding
    ) {
        self.keychain = keychain
        self.storeIdProvider = storeIdProvider
    }

    public convenience init(keychain: KeystoreProtocol, userDefaults: UserDefaults) {
        self.init(
            keychain: keychain,
            storeIdProvider: ProductResourceStoreIdStore(userDefaults: userDefaults)
        )
    }

    public func storeResourceKey(
        _ key: Data,
        for productId: ProductId,
        kind: ResourceKeyKind
    ) throws {
        let tag = keychainTag(for: productId, kind: kind)
        try keychain.saveKey(key, with: tag)
    }

    public func fetchResourceKey(
        for productId: ProductId,
        kind: ResourceKeyKind
    ) throws -> Data? {
        let tag = keychainTag(for: productId, kind: kind)

        guard try keychain.checkKey(for: tag) else {
            return nil
        }

        return try keychain.fetchKey(for: tag)
    }

    public func hasResourceKey(
        for productId: ProductId,
        kind: ResourceKeyKind
    ) throws -> Bool {
        let tag = keychainTag(for: productId, kind: kind)
        return try keychain.checkKey(for: tag)
    }
}

// MARK: - Private

private extension ProductResourceKeyManager {
    static let domain = "io.polkadotapp"
    static let namespace = "product.resource"

    func keychainTag(for productId: ProductId, kind: ResourceKeyKind) -> String {
        [
            Self.domain,
            storeIdProvider.getStoreId(),
            Self.namespace,
            productId,
            kind.rawValue
        ]
        .joined(separator: ":")
    }
}
