import Foundation

public protocol ContentHashCacheProtocol {
    func getContentHash(name: String) -> String?
    func putContentHash(name: String, hash: String)
    func removeContentHash(name: String)
    func clearAll()
}

public final class ContentHashCache: ContentHashCacheProtocol {
    public static let shared = ContentHashCache()

    public static let suiteName = "io.products.dotns.cache"
    private let defaults: UserDefaults

    public init() {
        guard let defaults = UserDefaults(suiteName: Self.suiteName) else {
            fatalError("Can't create user defaults. Check suite name")
        }

        self.defaults = defaults
    }

    public func getContentHash(name: String) -> String? {
        defaults.string(forKey: name)
    }

    public func putContentHash(name: String, hash: String) {
        defaults.set(hash, forKey: name)
    }

    public func removeContentHash(name: String) {
        defaults.removeObject(forKey: name)
    }

    public func clearAll() {
        defaults.removePersistentDomain(forName: Self.suiteName)
    }
}
