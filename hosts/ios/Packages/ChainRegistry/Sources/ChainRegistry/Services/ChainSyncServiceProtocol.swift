import Foundation

public protocol ChainSyncServiceProtocol {
    func syncUpChains()
    func updateLocal(chain: ChainModel)
}
