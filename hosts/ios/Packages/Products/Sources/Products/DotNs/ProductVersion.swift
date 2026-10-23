import Foundation

public struct ProductVersion: Equatable, Sendable {
    public struct RecordKey: Hashable, Sendable {
        public let name: String
        public let key: String
    }

    public let productId: ProductId
    public let records: [RecordKey: DotNsSnapshotRecord]
    public let contentHashes: [String: String]
}
