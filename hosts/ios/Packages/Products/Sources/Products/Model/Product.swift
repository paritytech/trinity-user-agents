import Foundation

import struct TrUAPIHost.ProductId

public struct Product: Identifiable {
    public let id: ProductId
    public let name: String

    public init(id: ProductId, name: String) {
        self.id = id
        self.name = name
    }
}
