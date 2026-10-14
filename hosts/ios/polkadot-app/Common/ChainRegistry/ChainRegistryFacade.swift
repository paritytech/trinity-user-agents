import Foundation
import ChainRegistry

typealias ChainRegistryLazyClosure = () -> ChainRegistryProtocol

enum ChainRegistryFacade {
    static let sharedRegistry: ChainRegistryProtocol = ChainRegistryFactory.createDefaultRegistry()
}
