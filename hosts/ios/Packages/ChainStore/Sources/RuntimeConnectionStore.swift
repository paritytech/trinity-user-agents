import Foundation
import SubstrateSdk

public protocol RuntimeConnectionStoring {
    func getConnection() throws -> JSONRPCEngine
    func getRuntimeProvider() throws -> RuntimeCodingServiceProtocol
}

public struct ChainRegistryRuntimeConnectionStore {
    public let chainId: ChainId
    public let chainRegistry: ChainResourceProtocol

    public init(chainId: ChainId, chainRegistry: ChainResourceProtocol) {
        self.chainId = chainId
        self.chainRegistry = chainRegistry
    }
}

extension ChainRegistryRuntimeConnectionStore: RuntimeConnectionStoring {
    public func getConnection() throws -> JSONRPCEngine {
        try chainRegistry.getRpcConnectionOrError(for: chainId)
    }

    public func getRuntimeProvider() throws -> RuntimeCodingServiceProtocol {
        try chainRegistry.getRuntimeCodingServiceOrError(for: chainId)
    }
}

public struct StaticRuntimeConnectionStore {
    public let connection: JSONRPCEngine
    public let runtimeProvider: RuntimeCodingServiceProtocol

    public init(connection: JSONRPCEngine, runtimeProvider: RuntimeCodingServiceProtocol) {
        self.connection = connection
        self.runtimeProvider = runtimeProvider
    }
}

extension StaticRuntimeConnectionStore: RuntimeConnectionStoring {
    public func getConnection() throws -> JSONRPCEngine {
        connection
    }

    public func getRuntimeProvider() throws -> RuntimeCodingServiceProtocol {
        runtimeProvider
    }
}
