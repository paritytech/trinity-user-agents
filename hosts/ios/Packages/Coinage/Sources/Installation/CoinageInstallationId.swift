import Foundation
import SubstrateSdk

/// The RFC-0017 `page` a single app installation allocates its coins and vouchers under.
///
/// Every installation draws its own, so a reinstall can never re-issue a key an earlier one already
/// handed off.
public struct CoinageInstallationId: Hashable, Sendable {
    public static let sizeBytes = 32

    public let value: Data

    public init(value: Data) throws {
        guard value.count == Self.sizeBytes else {
            throw CoinageInstallationIdError.invalidLength(value.count)
        }
        self.value = value
    }

    public init(hex: String) throws {
        try self.init(value: Data(hexString: hex))
    }

    public static func random() throws -> CoinageInstallationId {
        try CoinageInstallationId(value: Data.randomOrError(of: sizeBytes))
    }

    /// Hex without prefix — the storage form and the first half of a ``CoinageKeyIndex`` identifier.
    public var hex: String {
        value.toHex()
    }

    /// Hex keeps the 32 bytes intact: the junction factory maps a `0x` segment straight to the raw
    /// chain code, where a decimal segment would be parsed as a number and any other text hashed.
    public var pageSegment: String {
        value.toHex(includePrefix: true)
    }
}

extension CoinageInstallationId: Comparable {
    public static func < (lhs: CoinageInstallationId, rhs: CoinageInstallationId) -> Bool {
        lhs.value.lexicographicallyPrecedes(rhs.value)
    }
}

extension CoinageInstallationId: CustomStringConvertible {
    public var description: String { pageSegment }
}

public enum CoinageInstallationIdError: Error, Equatable {
    case invalidLength(Int)
}
