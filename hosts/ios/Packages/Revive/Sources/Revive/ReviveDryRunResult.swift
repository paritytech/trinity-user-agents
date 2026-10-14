import BigInt
import Foundation
import SubstrateSdk

/// The pallet's `ContractResult` with the fields a call reads; the dynamic decoder ignores the rest.
///
/// pallet-revive renamed `gas_required` to `weight_required` in the same change that renamed the call's
/// `gas_limit`, and both runtimes are live, so either spelling is accepted.
struct ReviveDryRunResult {
    let weightRequired: Substrate.WeightV2
    let storageDeposit: ReviveStorageDeposit
    let result: Substrate.Result<ReviveExecResult, JSON>
}

extension ReviveDryRunResult: Decodable {
    enum CodingKeys: String, CodingKey {
        case weightRequired
        case gasRequired
        case storageDeposit
        case result
    }

    init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let weightKey: CodingKeys = container.contains(.weightRequired) ? .weightRequired : .gasRequired
        weightRequired = try container.decode(Substrate.WeightDecodable.self, forKey: weightKey).wrappedValue
        storageDeposit = try container.decode(ReviveStorageDeposit.self, forKey: .storageDeposit)
        result = try container.decode(Substrate.Result<ReviveExecResult, JSON>.self, forKey: .result)
    }
}

enum ReviveStorageDeposit: Decodable {
    case refund(BigUInt)
    case charge(BigUInt)

    init(from decoder: any Decoder) throws {
        var container = try decoder.unkeyedContainer()
        let type = try container.decode(String.self)
        let value = try container.decode(StringCodable<BigUInt>.self).wrappedValue

        switch type {
        case "Refund": self = .refund(value)
        case "Charge": self = .charge(value)
        default:
            throw DecodingError.dataCorrupted(
                .init(codingPath: container.codingPath, debugDescription: "Unsupported storage deposit \(type)")
            )
        }
    }

    var charged: BigUInt {
        switch self {
        case let .charge(value): value
        case .refund: .zero
        }
    }
}

/// The contract's answer: its output bytes and the flags, of which only the revert bit matters here.
struct ReviveExecResult: Decodable {
    static let revertFlag: UInt32 = 0x1

    let flags: ReviveReturnFlags
    let data: BytesCodable

    var isReverted: Bool { flags.bits & Self.revertFlag != 0 }
    var output: Data { data.wrappedValue }
}

struct ReviveReturnFlags: Decodable {
    @StringCodable var bits: UInt32
}
