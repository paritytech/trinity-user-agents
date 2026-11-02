import struct TrUAPIHost.ProductId
import Foundation

public protocol ProductFileProviding {
    func load(for productId: ProductId, relativePath: String) -> Data?
}
