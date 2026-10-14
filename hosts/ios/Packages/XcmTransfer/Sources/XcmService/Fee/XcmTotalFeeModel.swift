import Foundation
import ExtrinsicService

struct XcmTotalFeeModel {
    let origin: ExtrinsicFeeProtocol
    let crosschain: XcmFeeModelProtocol
}
