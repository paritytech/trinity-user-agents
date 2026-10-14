import Foundation
import SubstrateSdk
import AssetsManagement

extension AssetsPallet {
    struct MintCall<A: Codable>: Codable {
        enum CodingKeys: String, CodingKey {
            case assetId = "id"
            case beneficiary
            case amount
        }

        let assetId: A
        let beneficiary: MultiAddress
        @StringCodable var amount: Balance

        func runtimeCall(for palletName: String) -> RuntimeCall<Self> {
            RuntimeCall(
                moduleName: palletName,
                callName: "mint",
                args: self
            )
        }
    }
}
