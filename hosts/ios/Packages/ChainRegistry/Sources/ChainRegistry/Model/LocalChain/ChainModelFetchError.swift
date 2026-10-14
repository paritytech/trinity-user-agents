import Foundation

public enum ChainModelFetchError: Error {
    case noAsset(assetId: AssetModel.Id)
}
