import Foundation
import BandersnatchApi
import SubstrateSdk

public extension MembersPallet {
    enum RingExponent: UInt8, Decodable, Equatable {
        case r2e9 = 9
        case r2e10 = 10
        case r2e14 = 14

        public init(from decoder: any Decoder) throws {
            var container = try decoder.unkeyedContainer()
            let type = try container.decode(String.self)

            switch type {
            case "R2e9":
                self = .r2e9
            case "R2e10":
                self = .r2e10
            case "R2e14":
                self = .r2e14
            default:
                throw DecodingError.dataCorruptedError(
                    in: container,
                    debugDescription: "Unexpected RingExponent type \(type)"
                )
            }
        }
    }

    struct CollectionInfo: Decodable, Equatable {
        public let ringSize: RingExponent
        @OptionStringCodable public var selfInclusionDelay: UInt64?
    }
}

public extension MembersPallet.RingExponent {
    var domainSize: BandersnatchApi.RingDomainSize {
        switch self {
        case .r2e9: .domain11
        case .r2e10: .domain12
        case .r2e14: .domain16
        }
    }

    /// Provable ring capacity — the real anonymity-set size a proof can verify against:
    /// `2^exponent - RING_PIOP_OVERHEAD`. The overhead is `4 + Bandersnatch scalar MODULUS_BIT_SIZE (253)`,
    /// constant across domain sizes. r2e9 -> 255, r2e10 -> 767, r2e14 -> 16127.
    var ringCapacity: Int {
        (1 << Int(rawValue)) - Self.ringPiopOverhead
    }

    private static var ringPiopOverhead: Int { 257 }
}
