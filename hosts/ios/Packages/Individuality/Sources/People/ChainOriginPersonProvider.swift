import Foundation
import SubstrateSdk
import ChainStore

public final class ChainOriginPersonProvider: OriginPersonProviding {
    private let chainId: ChainId
    private let chainRegistry: ChainResourceProtocol
    private let keyResolver: BandersnatchKeyResolving

    public init(
        chainId: ChainId,
        chainRegistry: ChainResourceProtocol,
        keyResolver: BandersnatchKeyResolving
    ) {
        self.chainId = chainId
        self.chainRegistry = chainRegistry
        self.keyResolver = keyResolver
    }

    public func pickPersonOrigins() async throws -> [PersonOrigin] {
        try await makeProvider().pickPersonOrigins()
    }

    public func pickPersonOrigin() async throws -> PersonOrigin {
        try await makeProvider().pickPersonOrigin()
    }
}

private extension ChainOriginPersonProvider {
    func makeProvider() throws -> OriginPersonProvider {
        let connection = try chainRegistry.getRpcConnectionOrError(for: chainId)
        let runtimeProvider = try chainRegistry.getRuntimeCodingServiceOrError(for: chainId)
        return OriginPersonProvider(
            liteVrfManager: keyResolver.liteKeyManager,
            liteCollectionId: PeopleLitePallet.membersIdentifier,
            fullVrfManager: keyResolver.fullKeyManager,
            fullCollectionId: PeoplePallet.membersIdentifier,
            connection: connection,
            runtimeCodingService: runtimeProvider
        )
    }
}
