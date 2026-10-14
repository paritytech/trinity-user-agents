import Foundation

protocol ProductNameCaching: Sendable {
    func name(for domain: String) -> String?
    func store(name: String, for domain: String)
}

final class ProductNameCache: ProductNameCaching, @unchecked Sendable {
    private let cache = NSCache<NSString, NSString>()

    func name(for domain: String) -> String? {
        cache.object(forKey: domain as NSString) as String?
    }

    func store(name: String, for domain: String) {
        cache.setObject(name as NSString, forKey: domain as NSString)
    }
}
