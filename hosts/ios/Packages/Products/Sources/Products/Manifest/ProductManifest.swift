import Foundation

/// A resolved product together with the dotNS text records it was resolved from, so the same
/// resolution can be replayed later without the chain.
public struct ProductManifest: Hashable, Sendable {
    public struct RecordKey: Hashable, Sendable {
        public let name: String
        public let key: String
    }

    /// An absent record is a read that found nothing, which is a different fact from a record
    /// that was never read.
    public enum Record: Hashable, Sendable {
        case absent
        case value(String)

        public init(_ value: String?) {
            self = value.map { .value($0) } ?? .absent
        }
    }

    public let product: ResolvedProduct
    public let records: [RecordKey: Record]

    public init(product: ResolvedProduct, records: [RecordKey: Record]) {
        self.product = product
        self.records = records
    }
}
