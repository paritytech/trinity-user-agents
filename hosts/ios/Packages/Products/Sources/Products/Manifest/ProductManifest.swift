import Foundation

public struct ProductManifest: Hashable, Sendable {
    public let product: ResolvedProduct
    public let records: [ProductVersion.RecordKey: DotNsSnapshotRecord]

    public init(product: ResolvedProduct, records: [ProductVersion.RecordKey: DotNsSnapshotRecord]) {
        self.product = product
        self.records = records
    }
}
