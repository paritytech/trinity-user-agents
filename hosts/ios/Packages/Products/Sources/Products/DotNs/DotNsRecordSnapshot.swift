import Foundation

public enum DotNsSnapshotRecord: Hashable, Sendable {
    case absent
    case value(String)

    public init(_ value: String?) {
        self = value.map { .value($0) } ?? .absent
    }
}

public protocol DotNsRecordSnapshotProtocol: Sendable {
    func record(name: String, key: String) -> DotNsSnapshotRecord?
    func store(_ record: DotNsSnapshotRecord, name: String, key: String)
    func remove(name: String, key: String)
    func clearAll()
}

public final class DotNsRecordSnapshot: DotNsRecordSnapshotProtocol, @unchecked Sendable {
    public static let suiteName = "io.products.dotns.records"

    private enum Constants {
        static let keyPrefix = "record."
        static let valueField = "value"
    }

    private let defaults: UserDefaults

    public init(defaults: UserDefaults) {
        self.defaults = defaults
    }

    public func record(name: String, key: String) -> DotNsSnapshotRecord? {
        guard let entry = defaults.dictionary(forKey: storageKey(name: name, key: key)) else {
            return nil
        }

        return (entry[Constants.valueField] as? String).map(DotNsSnapshotRecord.value) ?? .absent
    }

    public func store(_ record: DotNsSnapshotRecord, name: String, key: String) {
        let entry: [String: String] =
            switch record {
            case .absent:
                [:]
            case let .value(value):
                [Constants.valueField: value]
            }

        defaults.set(entry, forKey: storageKey(name: name, key: key))
    }

    public func remove(name: String, key: String) {
        defaults.removeObject(forKey: storageKey(name: name, key: key))
    }

    public func clearAll() {
        defaults.dictionaryRepresentation().keys
            .filter { $0.hasPrefix(Constants.keyPrefix) }
            .forEach(defaults.removeObject(forKey:))
    }
}

private extension DotNsRecordSnapshot {
    func storageKey(name: String, key: String) -> String {
        "\(Constants.keyPrefix)\(name)|\(key)"
    }
}
