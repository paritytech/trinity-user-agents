import Foundation
import Operation_iOS
import ExtrinsicService

protocol XcmCrosschainFeeCalculating {
    func crossChainFeeWrapper(
        request: XcmUnweightedTransferRequest
    ) -> CompoundOperationWrapper<XcmFeeModelProtocol>
}
