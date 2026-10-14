import Foundation
import SubstrateSdk

public struct XcmTransferReserve {
    public let chain: ChainProtocol
    public let parachainId: ParaId?

    public init(chain: ChainProtocol, parachainId: ParaId?) {
        self.chain = chain
        self.parachainId = parachainId
    }
}
