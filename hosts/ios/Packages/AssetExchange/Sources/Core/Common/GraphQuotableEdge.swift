import Foundation
import Operation_iOS
import SubstrateSdk

public protocol GraphQuotableEdge: GraphWeightableEdgeProtocol where Node == ChainAssetId {
    func quote(amount: Balance, direction: AssetConversion.Direction) -> CompoundOperationWrapper<Balance>
}
