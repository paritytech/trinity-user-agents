import Foundation
import SubstrateSdk
import BigInt

public extension AssetsPallet {
    struct Transfer: Codable {
        enum CodingKeys: String, CodingKey {
            case assetId = "id"
            case target
            case amount
        }

        let assetId: JSON
        let target: MultiAddress
        @StringCodable var amount: BigUInt

        public init(assetId: JSON, target: MultiAddress, amount: BigUInt) {
            self.assetId = assetId
            self.target = target
            self.amount = amount
        }

        public static func codingPath(for moduleName: String) -> CallCodingPath {
            .init(moduleName: moduleName, callName: "transfer")
        }

        public func runtimeCall() -> RuntimeCall<Self> {
            let path = Self.codingPath(for: AssetsPallet.name)

            return RuntimeCall(
                moduleName: path.moduleName,
                callName: path.callName,
                args: self
            )
        }
    }

    struct TransferAll: Codable {
        enum CodingKeys: String, CodingKey {
            case assetId = "id"
            case target = "dest"
            case keepAlive = "keep_alive"
        }

        let assetId: JSON
        let target: MultiAddress
        let keepAlive: Bool

        public init(assetId: JSON, target: MultiAddress, keepAlive: Bool) {
            self.assetId = assetId
            self.target = target
            self.keepAlive = keepAlive
        }

        public static func codingPath(for moduleName: String) -> CallCodingPath {
            .init(moduleName: moduleName, callName: "transfer_all")
        }

        public func runtimeCall() -> RuntimeCall<Self> {
            let path = Self.codingPath(for: AssetsPallet.name)

            return RuntimeCall(
                moduleName: path.moduleName,
                callName: path.callName,
                args: self
            )
        }
    }
}
