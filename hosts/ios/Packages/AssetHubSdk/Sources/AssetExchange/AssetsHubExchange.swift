import Foundation
import Operation_iOS
import SubstrateSdk
import AssetExchange

public final class AssetsHubExchange {
    let swapFactory: AssetHubSwapOperationFactoryProtocol
    let host: AssetHubExchangeHostProtocol

    public init(host: AssetHubExchangeHostProtocol, swapFactory: AssetHubSwapOperationFactoryProtocol) {
        self.host = host
        self.swapFactory = swapFactory
    }

    private func availableDirectSwapConnections(
        using swapFactory: AssetHubSwapOperationFactoryProtocol
    ) -> CompoundOperationWrapper<[any AssetExchangableGraphEdge]> {
        let connectionsWrapper = swapFactory.availableDirections()

        let mappingOperation = ClosureOperation<[any AssetExchangableGraphEdge]> {
            let connections = try connectionsWrapper.targetOperation.extractNoCancellableResultData()

            return connections.flatMap { keyValue in
                let origin = keyValue.key

                return keyValue.value.map { destination in
                    AssetHubExchangeEdge(
                        origin: origin,
                        destination: destination,
                        quoteFactory: swapFactory,
                        host: self.host
                    )
                }
            }
        }

        mappingOperation.addDependency(connectionsWrapper.targetOperation)

        return connectionsWrapper.insertingTail(operation: mappingOperation)
    }
}

extension AssetsHubExchange: AssetsExchangeProtocol {
    public func availableDirectSwapConnections() -> CompoundOperationWrapper<[any AssetExchangableGraphEdge]> {
        availableDirectSwapConnections(using: swapFactory)
    }
}
