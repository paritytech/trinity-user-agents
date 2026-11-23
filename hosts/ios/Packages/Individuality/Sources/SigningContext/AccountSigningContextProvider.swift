import Foundation
import ChainStore
import SubstrateSdk
import SubstrateStorageQuery
import StructuredConcurrency

/// Contexts the runtimes prepend to a message before checking an account signature over it.
/// A `nil` context means the runtime does not publish one yet and still verifies the bare message.
public struct AccountSigningContexts: Sendable {
    public let peopleLite: Data?
    public let dotnsGateway: Data?

    public init(peopleLite: Data?, dotnsGateway: Data?) {
        self.peopleLite = peopleLite
        self.dotnsGateway = dotnsGateway
    }
}

public protocol AccountSigningContextProviding: Sendable {
    func fetchContexts() async throws -> AccountSigningContexts
}

public final class AccountSigningContextProvider: @unchecked Sendable {
    private let peopleChainId: ChainId
    private let assetHubChainId: ChainId
    private let chainRegistry: ChainResourceProtocol

    public init(
        peopleChainId: ChainId,
        assetHubChainId: ChainId,
        chainRegistry: ChainResourceProtocol
    ) {
        self.peopleChainId = peopleChainId
        self.assetHubChainId = assetHubChainId
        self.chainRegistry = chainRegistry
    }
}

extension AccountSigningContextProvider: AccountSigningContextProviding {
    public func fetchContexts() async throws -> AccountSigningContexts {
        async let peopleLite = signingContext(
            at: PeopleLitePallet.Constants.signingContext(),
            on: peopleChainId
        )
        async let dotnsGateway = signingContext(
            at: DotnsGatewayPallet.Constants.signingContext(),
            on: assetHubChainId
        )

        return try await AccountSigningContexts(peopleLite: peopleLite, dotnsGateway: dotnsGateway)
    }
}

private extension AccountSigningContextProvider {
    func signingContext(at path: ConstantCodingPath, on chainId: ChainId) async throws -> Data? {
        let runtimeProvider = try chainRegistry.getRuntimeCodingServiceOrError(for: chainId)
        let codingFactory = try await runtimeProvider.fetchCoderFactoryOperation().asyncExecute()

        guard codingFactory.hasConstant(for: path) else {
            return nil
        }

        let operation = StorageConstantOperation<BytesCodable>(path: path)
        operation.codingFactory = codingFactory

        return try await operation.asyncExecute().wrappedValue
    }
}
