import Foundation
import EventCenter

public protocol ChainRegistryEventVisiting: EventVisitorProtocol {
    func processChainSyncDidStart(event: ChainSyncDidStart)
    func processChainSyncDidComplete(event: ChainSyncDidComplete)
    func processChainSyncDidFail(event: ChainSyncDidFail)

    func processRuntimeCommonTypesSyncCompleted(event: RuntimeCommonTypesSyncCompleted)
    func processRuntimeChainTypesSyncCompleted(event: RuntimeChainTypesSyncCompleted)
    func processRuntimeChainMetadataSyncCompleted(event: RuntimeMetadataSyncCompleted)

    func processRuntimeCoderReady(event: RuntimeCoderCreated)
    func processRuntimeCoderCreationFailed(event: RuntimeCoderCreationFailed)
}

public extension ChainRegistryEventVisiting {
    func processChainSyncDidStart(event _: ChainSyncDidStart) {}
    func processChainSyncDidComplete(event _: ChainSyncDidComplete) {}
    func processChainSyncDidFail(event _: ChainSyncDidFail) {}

    func processRuntimeCommonTypesSyncCompleted(event _: RuntimeCommonTypesSyncCompleted) {}
    func processRuntimeChainTypesSyncCompleted(event _: RuntimeChainTypesSyncCompleted) {}
    func processRuntimeChainMetadataSyncCompleted(event _: RuntimeMetadataSyncCompleted) {}

    func processRuntimeCoderReady(event _: RuntimeCoderCreated) {}
    func processRuntimeCoderCreationFailed(event _: RuntimeCoderCreationFailed) {}
}
