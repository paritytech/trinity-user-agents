import Foundation

public enum DotNsSnapshotRecord: Hashable, Sendable {
    case absent
    case value(String)

    public init(_ value: String?) {
        self = value.map { .value($0) } ?? .absent
    }
}

public protocol ProductVersionStoring: Sendable {
    func version(of productId: ProductId) -> ProductVersion?
    func save(_ version: ProductVersion) throws
    func remove(productId: ProductId)
}

public final class ProductVersionFileStore: ProductVersionStoring, @unchecked Sendable {
    private enum Constants {
        static let format = 1
        static let fileExtension = "json"
        static let allowedFileNameCharacters = CharacterSet.alphanumerics.union(CharacterSet(charactersIn: "-._"))
    }

    private let directory: URL
    private let fileManager: FileManager

    public init(directory: URL, fileManager: FileManager = .default) {
        self.directory = directory
        self.fileManager = fileManager
    }

    public convenience init(fileManager: FileManager = .default) {
        let appSupport = fileManager.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        self.init(
            directory: appSupport.appendingPathComponent("DotNsContent").appendingPathComponent("Versions"),
            fileManager: fileManager
        )
    }

    public func version(of productId: ProductId) -> ProductVersion? {
        guard
            let url = fileURL(for: productId),
            let data = try? Data(contentsOf: url),
            let stored = try? JSONDecoder().decode(StoredVersion.self, from: data),
            stored.format == Constants.format
        else {
            return nil
        }

        return stored.version
    }

    public func save(_ version: ProductVersion) throws {
        guard let url = fileURL(for: version.productId) else {
            throw CocoaError(.fileWriteInvalidFileName)
        }

        let encoder = JSONEncoder()
        encoder.outputFormatting = .sortedKeys

        try fileManager.createDirectory(at: directory, withIntermediateDirectories: true)
        try encoder.encode(StoredVersion(version, format: Constants.format)).write(to: url, options: .atomic)
    }

    public func remove(productId: ProductId) {
        guard let url = fileURL(for: productId) else { return }

        try? fileManager.removeItem(at: url)
    }
}

private extension ProductVersionFileStore {
    func fileURL(for productId: ProductId) -> URL? {
        productId
            .addingPercentEncoding(withAllowedCharacters: Constants.allowedFileNameCharacters)
            .map { directory.appendingPathComponent($0).appendingPathExtension(Constants.fileExtension) }
    }
}

private struct StoredVersion: Codable {
    struct Record: Codable {
        let name: String
        let key: String
        let value: String?
    }

    let format: Int
    let productId: ProductId
    let records: [Record]
    let contentHashes: [String: String]

    init(_ version: ProductVersion, format: Int) {
        self.format = format
        productId = version.productId
        records = version.records
            .map { recordKey, record in
                switch record {
                case .absent:
                    Record(name: recordKey.name, key: recordKey.key, value: nil)
                case let .value(value):
                    Record(name: recordKey.name, key: recordKey.key, value: value)
                }
            }
            .sorted { ($0.name, $0.key) < ($1.name, $1.key) }
        contentHashes = version.contentHashes
    }

    var version: ProductVersion {
        ProductVersion(
            productId: productId,
            records: Dictionary(
                records.map { (ProductVersion.RecordKey(name: $0.name, key: $0.key), DotNsSnapshotRecord($0.value)) },
                uniquingKeysWith: { first, _ in first }
            ),
            contentHashes: contentHashes
        )
    }
}
