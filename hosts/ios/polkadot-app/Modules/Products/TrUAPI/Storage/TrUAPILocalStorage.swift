import Foundation

protocol TrUAPILocalStoring: AnyObject, Sendable {
    func read(key: String) throws -> Data?
    func write(key: String, value: Data) throws
    func clear(key: String) throws
}

/// UserDefaults-backed KV store for the TrUAPI core; values are stored as
/// raw `Data`. Product storage is product-scoped; core storage is
/// host-GLOBAL — its prefix must NOT include a product id, the auth-session
/// slot is shared across product cores.
final class TrUAPILocalStorage: TrUAPILocalStoring, @unchecked Sendable {
    private static let productKeyPrefix = "io.polkadotapp.truapi.product.store"
    private static let coreKeyPrefix = "io.polkadotapp.truapi.core"

    private let keyPrefix: String
    private let readPrefix: @Sendable (String) -> String
    private let defaults: UserDefaults

    convenience init(keyPrefix: String, defaults: UserDefaults) {
        self.init(keyPrefix: keyPrefix, defaults: defaults) { _ in keyPrefix }
    }

    private init(
        keyPrefix: String,
        defaults: UserDefaults,
        readPrefix: @escaping @Sendable (String) -> String
    ) {
        self.keyPrefix = keyPrefix
        self.readPrefix = readPrefix
        self.defaults = defaults
    }

    /// Reads go to the product the core named as the key's owner, which is
    /// another product on a granted foreign read. Writes and clears stay in
    /// `productId`'s store.
    static func createProductLocalStorage(
        productId: String,
        defaults: UserDefaults = .standard
    ) -> TrUAPILocalStorage {
        let ownPrefix = "\(productKeyPrefix).\(productId)"
        let ownId = ProductStorageKey.normalize(productId)
        return TrUAPILocalStorage(keyPrefix: ownPrefix, defaults: defaults) { key in
            guard let owner = ProductStorageKey.owner(of: key), owner != ownId else { return ownPrefix }
            return "\(productKeyPrefix).\(owner)"
        }
    }

    static func createCoreLocalStorage(
        defaults: UserDefaults = .standard
    ) -> TrUAPILocalStorage {
        TrUAPILocalStorage(keyPrefix: coreKeyPrefix, defaults: defaults)
    }

    func read(key: String) throws -> Data? {
        defaults.data(forKey: "\(readPrefix(key)).\(key)")
    }

    func write(key: String, value: Data) throws {
        defaults.set(value, forKey: storageKey(key))
    }

    func clear(key: String) throws {
        defaults.removeObject(forKey: storageKey(key))
    }
}

private extension TrUAPILocalStorage {
    func storageKey(_ key: String) -> String {
        "\(keyPrefix).\(key)"
    }
}

/// Mirrors `ProductStorageKey::decode` in truapi:
/// `truapi:product-storage:v1:<byte length>:<product id>:<key>`.
enum ProductStorageKey {
    private static let prefix = "truapi:product-storage:v1:"

    /// The owning product id, or nil when `key` has any other shape.
    static func owner(of key: String) -> String? {
        guard key.hasPrefix(prefix) else { return nil }
        let rest = key.utf8.dropFirst(prefix.utf8.count)
        guard let colon = rest.firstIndex(of: UInt8(ascii: ":")),
              let lengthText = String(bytes: rest[..<colon], encoding: .utf8),
              let length = Int(lengthText),
              length > 0
        else { return nil }
        let start = rest.index(after: colon)
        guard let end = rest.index(start, offsetBy: length, limitedBy: rest.endIndex),
              end < rest.endIndex,
              rest[end] == UInt8(ascii: ":")
        else { return nil }
        return String(rest[start ..< end])
    }

    /// Matches the core's `normalize_product_identifier`, the form written into keys.
    static func normalize(_ productId: String) -> String {
        productId.trimmingCharacters(in: .whitespacesAndNewlines)
            .precomposedStringWithCanonicalMapping
            .lowercased()
    }
}
