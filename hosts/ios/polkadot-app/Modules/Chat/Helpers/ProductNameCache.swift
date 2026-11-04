import Foundation
import struct TrUAPIHost.ProductId

protocol ProductNameCaching: Sendable {
    func name(for domain: ProductId) -> String?
    func store(name: String, for domain: ProductId)
}

final class ProductNameCache: ProductNameCaching, @unchecked Sendable {
    private let cache = NSCache<NSString, NSString>()

    func name(for domain: ProductId) -> String? {
        cache.object(forKey: domain.rawValue as NSString) as String?
    }

    func store(name: String, for domain: ProductId) {
        cache.setObject(name as NSString, forKey: domain.rawValue as NSString)
    }
}
