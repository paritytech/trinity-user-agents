import Foundation
import Operation_iOS
import SubstrateSdk

public enum AssetConversionOperationError: Error {
    case remoteAssetNotFound(ChainAssetId)
    case runtimeError(String)
    case quoteCalcFailed
    case tradeDisabled
    case noRoutesAvailable
}
