import Foundation
import Foundation_iOS
import SubstrateSdk
import HandoffService
import SDKLogger
import ChainRegistry

enum HOPFileLoaderError: Error {
    case invalidUrl
    case noAvailableNodes
    case untrustedNode
}

protocol HOPFileLoaderMaking {
    func makeLoader(for node: ChatRemoteMessageContent.NodeEndpoint) throws -> HandoffFileLoading
}

final class HOPFileLoaderFactory {
    let logger: SDKLoggerProtocol

    init(logger: SDKLoggerProtocol) {
        self.logger = logger
    }
}

extension HOPFileLoaderFactory: HOPFileLoaderMaking {
    func makeLoader(for node: ChatRemoteMessageContent.NodeEndpoint) throws -> HandoffFileLoading {
        let url = try node.toURL()

        guard let connection = WebSocketEngine(
            urls: [url],
            connectionFactory: ConnectionTransportFactory(),
            logger: logger
        ) else {
            throw HOPFileLoaderError.noAvailableNodes
        }

        let service = HandoffService(connection: connection)
        let remoteStore = BitswapRemoteStore(connection: connection, logger: logger)
        let decoratedService = HandoffServiceDecorator(
            handoffService: service,
            remoteStore: remoteStore
        )

        return HandoffFileLoader(service: decoratedService, config: HandoffFileLoadConfig())
    }
}

// MARK: - HOP Node Provider

protocol HOPNodeProviding {
    func selectNode() -> ChatRemoteMessageContent.NodeEndpoint?
    func isNodeAllowed(_ node: ChatRemoteMessageContent.NodeEndpoint) -> Bool
}

extension HOPNodeProviding {
    func selectNodeOrError() throws -> ChatRemoteMessageContent.NodeEndpoint {
        try selectNode().mapOrThrow(HOPFileLoaderError.noAvailableNodes)
    }
}

final class HOPNodeProvider {
    let chainRegistry: ChainRegistryProtocol
    let chainId: ChainModel.Id

    init(
        chainRegistry: ChainRegistryProtocol,
        chainId: ChainModel.Id = AppConfig.Chains.bulletInChain
    ) {
        self.chainRegistry = chainRegistry
        self.chainId = chainId
    }
}

extension HOPNodeProvider: HOPNodeProviding {
    func selectNode() -> ChatRemoteMessageContent.NodeEndpoint? {
        guard let api = hopApis()?.randomElement() else {
            return nil
        }

        return .wssUrl(api.url.absoluteString)
    }

    func isNodeAllowed(_ node: ChatRemoteMessageContent.NodeEndpoint) -> Bool {
        guard let url = try? node.toURL(),
              let apis = hopApis() else {
            return false
        }

        return apis.contains { $0.url == url }
    }
}

private extension HOPNodeProvider {
    func hopApis() -> Set<LocalChainExternalApi>? {
        chainRegistry.getChain(for: chainId)?.externalApis?.hop()
    }
}
