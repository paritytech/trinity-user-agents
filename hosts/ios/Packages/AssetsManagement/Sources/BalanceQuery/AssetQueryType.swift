import BigInt

public enum AssetQueryType {
    case native
    case statemine(assetId: String, palletName: String?)
    case orml(currencyIdScale: String)
    case hydrationEvm(remoteAssetId: BigUInt)
}
