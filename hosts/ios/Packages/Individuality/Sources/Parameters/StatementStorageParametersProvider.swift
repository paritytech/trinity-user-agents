import Foundation
import SubstrateSdk
import SubstrateStorageQuery
import ChainStore
import StructuredConcurrency

public protocol StatementStorageParametersProviding: Sendable {
    /// Constant on old runtimes, `pallet_parameters` value afterwards; `nil` while the runtime default applies.
    func notificationAllowance() async throws -> ResourcesPallet.StatementAllowance?
}

public extension ResourcesPallet.StatementAllowance {
    /// Applies while the parameter is unset and no metadata exposes it.
    static let notificationDefault = ResourcesPallet.StatementAllowance(maxCount: 1, maxSize: 5 * 1_024 / 2)
}

public final class StatementStorageParametersProvider: StatementStorageParametersProviding, @unchecked Sendable {
    private let chainId: ChainId
    private let chainRegistry: ChainResourceProtocol
    private let storageRequestFactory: StorageRequestFactoryProtocol

    public init(
        chainId: ChainId,
        chainRegistry: ChainResourceProtocol,
        storageRequestFactory: StorageRequestFactoryProtocol
    ) {
        self.chainId = chainId
        self.chainRegistry = chainRegistry
        self.storageRequestFactory = storageRequestFactory
    }

    public func notificationAllowance() async throws -> ResourcesPallet.StatementAllowance? {
        let runtimeProvider = try chainRegistry.getRuntimeCodingServiceOrError(for: chainId)
        let codingFactory = try await runtimeProvider.fetchCoderFactoryOperation().asyncExecute()

        let constantPath = ResourcesPallet.Constants.notificationAllowance()

        if codingFactory.getConstant(for: constantPath) != nil {
            return try await runtimeProvider.fetchConstant(path: constantPath)
        }

        return try await parameter(codingFactory: codingFactory)
    }
}

private extension StatementStorageParametersProvider {
    func parameter(codingFactory: RuntimeCoderFactoryProtocol) async throws -> ResourcesPallet.StatementAllowance? {
        let connection = try chainRegistry.getRpcConnectionOrError(for: chainId)

        let key = ParametersPallet.StatementStorageKey(parameter: .notificationAllowance)
        let responses: [StorageResponse<ParametersPallet.StatementStorageValue<ResourcesPallet.StatementAllowance>>]
        responses = try await storageRequestFactory.queryItems(
            engine: connection,
            keyParams: { [key] },
            factory: { codingFactory },
            storagePath: ParametersPallet.Storage.parameters()
        )
        .asyncExecute()

        return responses.first?.value?.value
    }
}
