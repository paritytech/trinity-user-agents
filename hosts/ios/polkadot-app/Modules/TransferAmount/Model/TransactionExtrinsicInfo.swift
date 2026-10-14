import Foundation
import SubstrateSdk

struct TransactionExtrinsicInfo {
    let blockNumber: BlockNumber
    let blockHash: Data
    let extrinsicIndex: ExtrinsicIndex
    let extrinsicHash: Data
}
