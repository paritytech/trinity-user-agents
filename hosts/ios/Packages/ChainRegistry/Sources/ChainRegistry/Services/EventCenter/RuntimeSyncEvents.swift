import Foundation
import EventCenter
import SubstrateSdkExt

public struct RuntimeCommonTypesSyncCompleted: EventProtocol {
    public let fileHash: String

    public init(fileHash: String) {
        self.fileHash = fileHash
    }

    public func accept(visitor: EventVisitorProtocol) {
        (visitor as? ChainRegistryEventVisiting)?.processRuntimeCommonTypesSyncCompleted(event: self)
    }
}

public struct RuntimeChainTypesSyncCompleted: EventProtocol {
    public let chainId: ChainModel.Id
    public let fileHash: String

    public init(chainId: ChainModel.Id, fileHash: String) {
        self.chainId = chainId
        self.fileHash = fileHash
    }

    public func accept(visitor: EventVisitorProtocol) {
        (visitor as? ChainRegistryEventVisiting)?.processRuntimeChainTypesSyncCompleted(event: self)
    }
}

public struct RuntimeMetadataSyncCompleted: EventProtocol {
    public let chainId: ChainModel.Id
    public let version: RuntimeVersion

    public init(chainId: ChainModel.Id, version: RuntimeVersion) {
        self.chainId = chainId
        self.version = version
    }

    public func accept(visitor: EventVisitorProtocol) {
        (visitor as? ChainRegistryEventVisiting)?.processRuntimeChainMetadataSyncCompleted(event: self)
    }
}
