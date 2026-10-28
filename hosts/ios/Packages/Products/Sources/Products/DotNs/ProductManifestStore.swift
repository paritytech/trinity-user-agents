import Foundation
import os

/// Persists the records a product's manifest was last resolved from. Archives are not part of it:
/// each executable's applied archive is tracked by name in the content hash cache.
public protocol ProductManifestStoring: Sendable {
    func records(of productId: ProductId) -> [ProductManifest.RecordKey: ProductManifest.Record]?
    /// Replaces the given records and keeps the rest, so one executable's record can change without
    /// the others.
    func save(_ records: [ProductManifest.RecordKey: ProductManifest.Record]) throws
}

public final class ProductManifestFileStore: ProductManifestStoring, @unchecked Sendable {
    private enum Constants {
        static let format = 1
        static let fileExtension = "json"
        static let allowedFileNameCharacters = CharacterSet.alphanumerics.union(CharacterSet(charactersIn: "-._"))
    }

    /// Process-wide: every store instance merges into the same files.
    private static let writeLock = OSAllocatedUnfairLock()

    private let directory: URL
    private let fileManager: FileManager

    public init(directory: URL, fileManager: FileManager = .default) {
        self.directory = directory
        self.fileManager = fileManager
    }

    public convenience init(fileManager: FileManager = .default) {
        let appSupport = fileManager.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        self.init(
            directory: appSupport.appendingPathComponent("DotNsContent").appendingPathComponent("Manifests"),
            fileManager: fileManager
        )
    }

    public func records(of productId: ProductId) -> [ProductManifest.RecordKey: ProductManifest.Record]? {
        guard
            let url = fileURL(for: productId),
            let data = try? Data(contentsOf: url),
            let stored = try? JSONDecoder().decode(StoredManifest.self, from: data),
            stored.format == Constants.format
        else {
            return nil
        }

        return stored.manifestRecords
    }

    public func save(_ records: [ProductManifest.RecordKey: ProductManifest.Record]) throws {
        let recordsByProduct = Dictionary(grouping: records) { ProductManifestRecords.baseName(of: $0.key.name) }

        try Self.writeLock.withLock {
            for (productId, productRecords) in recordsByProduct {
                let merged = (self.records(of: productId) ?? [:])
                    .merging(productRecords.map { ($0.key, $0.value) }) { _, new in new }

                try write(merged, of: productId)
            }
        }
    }
}

private extension ProductManifestFileStore {
    func write(_ records: [ProductManifest.RecordKey: ProductManifest.Record], of productId: ProductId) throws {
        guard let url = fileURL(for: productId) else {
            throw CocoaError(.fileWriteInvalidFileName)
        }

        let encoder = JSONEncoder()
        encoder.outputFormatting = .sortedKeys

        try fileManager.createDirectory(at: directory, withIntermediateDirectories: true)
        try encoder.encode(StoredManifest(records, format: Constants.format)).write(to: url, options: .atomic)
    }

    func fileURL(for productId: ProductId) -> URL? {
        productId
            .addingPercentEncoding(withAllowedCharacters: Constants.allowedFileNameCharacters)
            .map { directory.appendingPathComponent($0).appendingPathExtension(Constants.fileExtension) }
    }
}

private struct StoredManifest: Codable {
    struct Record: Codable {
        let name: String
        let key: String
        let value: String?
    }

    let format: Int
    let records: [Record]

    init(_ manifestRecords: [ProductManifest.RecordKey: ProductManifest.Record], format: Int) {
        self.format = format
        records = manifestRecords
            .map { recordKey, record in
                switch record {
                case .absent:
                    Record(name: recordKey.name, key: recordKey.key, value: nil)
                case let .value(value):
                    Record(name: recordKey.name, key: recordKey.key, value: value)
                }
            }
            .sorted { ($0.name, $0.key) < ($1.name, $1.key) }
    }

    var manifestRecords: [ProductManifest.RecordKey: ProductManifest.Record] {
        Dictionary(
            records.map { (ProductManifest.RecordKey(name: $0.name, key: $0.key), ProductManifest.Record($0.value)) },
            uniquingKeysWith: { first, _ in first }
        )
    }
}
