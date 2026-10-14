import Foundation
import SubstrateSdk
import AssetsManagement

extension AssetsPallet {
    static func issuedPath(for moduleName: String?) -> EventCodingPath {
        EventCodingPath(moduleName: moduleName ?? AssetsPallet.name, eventName: "Issued")
    }

    static func depositedPath(for moduleName: String?) -> EventCodingPath {
        EventCodingPath(moduleName: moduleName ?? AssetsPallet.name, eventName: "Deposited")
    }
}

extension AssetsPallet {
    struct IssuedEvent: Decodable {
        let assetId: JSON
        let accountId: AccountId
        let amount: Balance

        init(from decoder: Decoder) throws {
            var unkeyedContainer = try decoder.unkeyedContainer()

            assetId = try unkeyedContainer.decode(JSON.self)
            accountId = try unkeyedContainer.decode(BytesCodable.self).wrappedValue
            amount = try unkeyedContainer.decode(StringScaleMapper<Balance>.self).value
        }
    }
}
