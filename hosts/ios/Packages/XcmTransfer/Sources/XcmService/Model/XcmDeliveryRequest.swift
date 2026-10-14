import Foundation
import SubstrateSdk
import XcmDefinition

struct XcmDeliveryRequest {
    let message: XcmUni.VersionedMessage
    let fromChainId: ChainId
    let toParachainId: ParaId?
}
