import CryptoKit
import Foundation

protocol TrUAPILocalStoring: AnyObject, Sendable {
    func read(key: String) throws -> Data?
    func write(key: String, value: Data) throws
    func clear(key: String) throws
}

/// KV store for the TrUAPI core; values are raw `Data`. Product storage is
/// product-scoped; core storage is host-GLOBAL — its prefix must NOT include
/// a product id, the auth-session slot is shared across product cores.
///
/// Core storage stays in `UserDefaults` (a few small values). Product storage
/// is one file per key under `directory`: a product can keep megabytes (the
/// image of a database), and CFPreferences refuses every write once the app's
/// `UserDefaults` domain passes 4 MiB ("Attempting to store >= 4194304 bytes
/// of data in CFPreferences/NSUserDefaults on this platform is invalid"). The
/// refusal is silent: `set` returns, the value is not stored, and a product
/// that wrote it reads back nothing. Values that an earlier build stored in
/// `UserDefaults` are read from there until they move to files (see
/// ``migrateLegacyProductValues(defaults:directory:)``).
final class TrUAPILocalStorage: TrUAPILocalStoring, @unchecked Sendable {
    private static let productKeyPrefix = "io.polkadotapp.truapi.product.store"
    private static let coreKeyPrefix = "io.polkadotapp.truapi.core"
    /// The marker of a key that the core writes for a product.
    private static let productKeyMarker = ".truapi:product-storage:v1:"

    private let keyPrefix: String
    private let readPrefix: @Sendable (String) -> String
    private let defaults: UserDefaults
    /// The root of the product files. `nil` for core storage, which stays in
    /// `defaults`.
    private let directory: URL?

    convenience init(keyPrefix: String, defaults: UserDefaults) {
        self.init(keyPrefix: keyPrefix, defaults: defaults, directory: nil) { _ in keyPrefix }
    }

    private init(
        keyPrefix: String,
        defaults: UserDefaults,
        directory: URL?,
        readPrefix: @escaping @Sendable (String) -> String
    ) {
        self.keyPrefix = keyPrefix
        self.readPrefix = readPrefix
        self.defaults = defaults
        self.directory = directory
    }

    /// The directory of product storage files of the app.
    static var defaultProductDirectory: URL {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("TrUAPIProductStorage", isDirectory: true)
    }

    /// Reads go to the product the core named as the key's owner, which is
    /// another product on a granted foreign read. Writes and clears stay in
    /// `productId`'s store.
    static func createProductLocalStorage(
        productId: String,
        defaults: UserDefaults = .standard,
        directory: URL = defaultProductDirectory
    ) -> TrUAPILocalStorage {
        migrateLegacyProductValues(defaults: defaults, directory: directory)
        let ownPrefix = "\(productKeyPrefix).\(productId)"
        let ownId = ProductStorageKey.normalize(productId)
        return TrUAPILocalStorage(keyPrefix: ownPrefix, defaults: defaults, directory: directory) { key in
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
        let prefix = readPrefix(key)
        if let directory {
            if let data = try Self.readFile(Self.fileURL(directory, prefix: prefix, key: key)) {
                return data
            }
        }
        return defaults.data(forKey: "\(prefix).\(key)")
    }

    func write(key: String, value: Data) throws {
        guard let directory else {
            defaults.set(value, forKey: storageKey(key))
            return
        }
        try Self.writeFile(Self.fileURL(directory, prefix: keyPrefix, key: key), value)
        // A value of an earlier build must not show again after a clear.
        removeLegacyValue(key)
    }

    func clear(key: String) throws {
        if let directory {
            try Self.removeFile(Self.fileURL(directory, prefix: keyPrefix, key: key))
        }
        removeLegacyValue(key)
    }

    // MARK: - Legacy values in UserDefaults

    /// The directories that a migration in this process went through.
    private final class Migrations: @unchecked Sendable {
        let lock = NSLock()
        var done = Set<String>()
    }

    private static let migrations = Migrations()

    /// Move every product value that an earlier build stored in `defaults` to
    /// files under `directory`, once for each directory in a process. This
    /// takes the app's `UserDefaults` domain back under the 4 MiB that
    /// CFPreferences accepts, so the core storage there works again. A value
    /// whose file is there already stays as the file has it.
    static func migrateLegacyProductValues(defaults: UserDefaults, directory: URL) {
        migrations.lock.withLock {
            guard migrations.done.insert(directory.standardizedFileURL.path).inserted else {
                return
            }
            for (name, value) in defaults.dictionaryRepresentation() {
                guard name.hasPrefix(productKeyPrefix + "."),
                      let data = value as? Data,
                      let marker = name.range(of: productKeyMarker)
                else { continue }
                let prefix = String(name[..<marker.lowerBound])
                let key = String(name[name.index(after: marker.lowerBound)...])
                let url = fileURL(directory, prefix: prefix, key: key)
                do {
                    if try readFile(url) == nil {
                        try writeFile(url, data)
                    }
                    defaults.removeObject(forKey: name)
                } catch {
                    // The value stays in `defaults`, and reads still find it.
                    continue
                }
            }
        }
    }

    private func removeLegacyValue(_ key: String) {
        let name = storageKey(key)
        if defaults.object(forKey: name) != nil {
            defaults.removeObject(forKey: name)
        }
    }

    // MARK: - Files

    /// `<directory>/<SHA-256 of the store prefix>/<SHA-256 of the key>`: the
    /// names hold no product id or key in clear, and any key fits.
    private static func fileURL(_ directory: URL, prefix: String, key: String) -> URL {
        directory
            .appendingPathComponent(sha256Hex(prefix), isDirectory: true)
            .appendingPathComponent(sha256Hex(key), isDirectory: false)
    }

    private static func sha256Hex(_ text: String) -> String {
        SHA256.hash(data: Data(text.utf8)).map { String(format: "%02x", $0) }.joined()
    }

    /// The value in `url`, or `nil` when there is no file. Any other failure
    /// throws, so that a value that cannot be read never looks absent.
    private static func readFile(_ url: URL) throws -> Data? {
        do {
            return try Data(contentsOf: url)
        } catch let error as CocoaError where error.code == .fileReadNoSuchFile {
            return nil
        }
    }

    /// Write `value` to `url` as one step: the file has the old value or the
    /// new one, also after a crash.
    private static func writeFile(_ url: URL, _ value: Data) throws {
        try FileManager.default.createDirectory(
            at: url.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        try value.write(to: url, options: .atomic)
    }

    private static func removeFile(_ url: URL) throws {
        do {
            try FileManager.default.removeItem(at: url)
        } catch let error as CocoaError where error.code == .fileNoSuchFile {
            return
        }
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
