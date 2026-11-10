import Foundation

/// Locates one own coin or voucher key: the installation subtree it was allocated in, and its item
/// there.
public struct CoinageKeyIndex: Hashable, Sendable {
    private static let separator: Character = "/"

    public let installation: CoinageInstallationId
    public let item: DerivationIndex

    public init(installation: CoinageInstallationId, item: DerivationIndex) {
        self.installation = installation
        self.item = item
    }

    /// `"{installationHex}/{item}"`: the row identifier of the coin or voucher and the form the index
    /// is persisted in where a column holds a list of them.
    public func toString() -> String {
        "\(installation.hex)\(Self.separator)\(item)"
    }

    public static func fromString(_ string: String) throws -> CoinageKeyIndex {
        let parts = string.split(separator: separator, maxSplits: 1)
        guard parts.count == 2, let item = DerivationIndex(parts[1]) else {
            throw CoinageKeyIndexError.malformed(string)
        }
        return try CoinageKeyIndex(installation: CoinageInstallationId(hex: String(parts[0])), item: item)
    }
}

public enum CoinageKeyIndexError: Error, Equatable {
    case malformed(String)
}

extension CoinageKeyIndex: Comparable {
    public static func < (lhs: CoinageKeyIndex, rhs: CoinageKeyIndex) -> Bool {
        if lhs.installation != rhs.installation {
            return lhs.installation < rhs.installation
        }
        return lhs.item < rhs.item
    }
}

extension CoinageKeyIndex: CustomStringConvertible {
    public var description: String { toString() }
}
