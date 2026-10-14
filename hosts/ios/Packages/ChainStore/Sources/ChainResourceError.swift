import Foundation
import SubstrateSdk

public enum ChainResourceError: Error {
    case connectionUnavailable
    case runtimeMetadaUnavailable
    case noChain(ChainId)
    case noChainGenesis(String)
}
