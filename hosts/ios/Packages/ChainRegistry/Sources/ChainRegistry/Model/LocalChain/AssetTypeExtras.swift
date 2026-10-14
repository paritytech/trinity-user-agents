import Foundation
import SubstrateSdk

public struct StatemineAssetExtras: Codable {
    public let assetId: String
    public let palletName: String?

    public init(assetId: String, palletName: String?) {
        self.assetId = assetId
        self.palletName = palletName
    }
}

public struct OrmlTokenExtras: Codable {
    public let currencyIdScale: String
    public let currencyIdType: String
    public let existentialDeposit: String
    public let transfersEnabled: Bool?

    public init(
        currencyIdScale: String,
        currencyIdType: String,
        existentialDeposit: String,
        transfersEnabled: Bool?
    ) {
        self.currencyIdScale = currencyIdScale
        self.currencyIdType = currencyIdType
        self.existentialDeposit = existentialDeposit
        self.transfersEnabled = transfersEnabled
    }
}

public typealias AssetTypeExtras = JSON
