import Foundation
import SubstrateSdk
import BigInt

public struct XcmDeliveryFee: Decodable {
    public struct FeeType: Decodable {
        public let type: String

        public init(type: String) {
            self.type = type
        }
    }

    public struct Exponential: Decodable {
        public let factorPallet: String
        @StringCodable public var sizeBase: BigUInt
        @StringCodable public var sizeFactor: BigUInt
        public let alwaysHoldingPays: Bool?

        public var isSenderPaysOriginDelivery: Bool {
            !(alwaysHoldingPays ?? false)
        }

        public var parachainFactorStoragePath: StorageCodingPath {
            StorageCodingPath(moduleName: factorPallet, itemName: "DeliveryFeeFactor")
        }

        public var upwardFactorStoragePath: StorageCodingPath {
            StorageCodingPath(moduleName: factorPallet, itemName: "UpwardDeliveryFeeFactor")
        }
    }

    public enum Price: Decodable {
        case exponential(Exponential)
        case undefined

        public init(from decoder: Decoder) throws {
            let feeType = try FeeType(from: decoder).type

            switch feeType {
            case "exponential":
                let value = try Exponential(from: decoder)
                self = .exponential(value)
            default:
                self = .undefined
            }
        }

        public var alwaysHoldingPays: Bool? {
            switch self {
            case let .exponential(exponential):
                exponential.alwaysHoldingPays
            default:
                nil
            }
        }
    }

    public let toParent: Price?
    public let toParachain: Price?
}
