import Foundation
import SubstrateSdk
import SubstrateSdkExt

public extension PGASPallet {
    /// call_index: 0
    /// Origin: ClaimAlias(alias, day, collection) via AsPgas(Claim(..))
    /// slotIndex: 0..MaxClaimsPerPeriodPerPerson-1; each index produces distinct alias
    struct ClaimPgasCall: RuntimeCallConvertible {
        enum CodingKeys: String, CodingKey {
            case slotIndex = "slot_index"
            case target
        }

        public var moduleName: String { PGASPallet.name }
        public var name: String { "claim_pgas" }

        @StringCodable public var slotIndex: UInt32
        @BytesCodable public var target: Data

        public init(slotIndex: UInt32, target: Data) {
            _slotIndex = StringCodable(wrappedValue: slotIndex)
            _target = BytesCodable(wrappedValue: target)
        }
    }
}
