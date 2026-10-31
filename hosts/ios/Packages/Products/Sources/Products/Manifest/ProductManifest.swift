import Foundation

public struct ProductManifest: Hashable, Sendable {
    public struct RecordKey: Hashable, Sendable {
        public let name: String
        public let key: String
    }

    /// `absent` is a read that found nothing; a missing key was never read.
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
