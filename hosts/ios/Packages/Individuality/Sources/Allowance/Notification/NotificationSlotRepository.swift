import ChainStore
import Foundation
import SubstrateSdk
import SubstrateSdkExt
import SubstrateStorageQuery

/// Chain reads behind notification slot claims, all on the chain the slots live on.
public protocol NotificationSlotRepositoryProtocol: Sendable {
    /// False on a runtime without notification slots.
    func isSupported() async throws -> Bool

    func highestSeq(for origin: PersonOrigin) async throws -> UInt8

    /// The subset of `aliases` already bound to an account this period.
    func registeredAliases(_ aliases: [Data]) async throws -> Set<Data>
}

public final class NotificationSlotRepository: @unchecked Sendable {
    private let chainId: ChainId
    private let chainRegistry: ChainResourceProtocol
    private let storageRequestFactory: StorageRequestFactoryProtocol
    private let resourcesParameters: ResourcesParametersProviding

    public init(
        chainId: ChainId,
        chainRegistry: ChainResourceProtocol,
        storageRequestFactory: StorageRequestFactoryProtocol,
        resourcesParameters: ResourcesParametersProviding
    ) {
        self.chainId = chainId
        self.chainRegistry = chainRegistry
        self.storageRequestFactory = storageRequestFactory
        self.resourcesParameters = resourcesParameters
    }
}

extension NotificationSlotRepository: NotificationSlotRepositoryProtocol {
    public func isSupported() async throws -> Bool {
        let path = ResourcesPallet.notificationRegistrationByAlias

        return try await codingFactory().metadata.getStorageMetadata(
            in: path.moduleName,
            storageName: path.itemName
        ) != nil
    }

    public func highestSeq(for origin: PersonOrigin) async throws -> UInt8 {
        try await resourcesParameters.notificationHighestSeq(chainId: chainId, origin: origin)
    }

    public func registeredAliases(_ aliases: [Data]) async throws -> Set<Data> {
        guard !aliases.isEmpty else { return [] }

        let connection = try chainRegistry.getRpcConnectionOrError(for: chainId)
        let codingFactory = try await codingFactory()

        let responses: [StorageResponse<JSON>] = try await storageRequestFactory.queryItems(
            engine: connection,
            keyParams: { aliases.map { BytesCodable(wrappedValue: $0) } },
            factory: { codingFactory },
            storagePath: ResourcesPallet.notificationRegistrationByAlias
        )
        .asyncExecute()

        return Set(zip(aliases, responses).compactMap { alias, response in
            response.value == nil ? nil : alias
        })
    }
}

private extension NotificationSlotRepository {
    func codingFactory() async throws -> RuntimeCoderFactoryProtocol {
        let runtimeProvider = try chainRegistry.getRuntimeCodingServiceOrError(for: chainId)

        return try await runtimeProvider.fetchCoderFactoryOperation().asyncExecute()
    }
}
