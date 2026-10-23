import Foundation

public struct ProductVersion: Equatable, Sendable {
    public struct RecordKey: Hashable, Sendable {
        public let name: String
        public let key: String
    }

    public let productId: ProductId
    public let records: [RecordKey: DotNsSnapshotRecord]
    public let contentHashes: [String: String]

    public init(productId: ProductId, records: [RecordKey: DotNsSnapshotRecord], contentHashes: [String: String]) {
        self.productId = productId
        self.records = records
        self.contentHashes = contentHashes
    }
}
