import Foundation
import SubstrateSdk

public enum OriginRestrictionPallet {}

public extension OriginRestrictionPallet {
    struct TransactionExtension: Codable {
        public let enabled: Bool

        public init(enabled: Bool) {
            self.enabled = enabled
        }

        public init(from decoder: any Decoder) throws {
            enabled = try Bool(from: decoder)
        }

        public func encode(to encoder: any Encoder) throws {
            try enabled.encode(to: encoder)
        }
    }
}

extension OriginRestrictionPallet.TransactionExtension: OnlyExplicitTransactionExtending {
    public var txExtensionId: String { "RestrictOrigins" }
}
