import Foundation
import SubstrateSdkExt

extension RuntimeSyncService: RuntimeSyncServiceProtocol {
    public func register(chain: ChainModel, with connection: ChainConnection) {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        guard let syncInfo = knownChains[chain.chainId] else {
            knownChains[chain.chainId] = SyncInfo(typesURL: chain.types?.url, connection: connection)
            return
        }

        if syncInfo.typesURL != chain.types?.url {
            knownChains[chain.chainId] = SyncInfo(typesURL: chain.types?.url, connection: connection)

            performSync(for: chain.chainId, shouldSyncTypes: true)
        }
    }

    public func unregisterIfExists(chainId: ChainModel.Id) {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        clearOperations(for: chainId)
        knownChains[chainId] = nil
    }

    public func apply(version: RuntimeVersion, for chainId: ChainModel.Id) {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        clearOperations(for: chainId)

        performSync(for: chainId, shouldSyncTypes: true, newVersion: version)
    }

    public func hasChain(with chainId: ChainModel.Id) -> Bool {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        return knownChains[chainId] != nil
    }

    public func isChainSyncing(_ chainId: ChainModel.Id) -> Bool {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        return (syncingChains[chainId] != nil) || (retryAttempts[chainId] != nil)
    }
}
