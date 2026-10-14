import Foundation
import Operation_iOS
import SubstrateSdk

public protocol BlockHashOperationFactoryProtocol {
    func createBlockHashOperation(
        connection: JSONRPCEngine,
        for numberClosure: @escaping () throws -> BlockNumber
    ) -> BaseOperation<String>
}

public final class BlockHashOperationFactory: BlockHashOperationFactoryProtocol {
    public init() {}

    public func createBlockHashOperation(
        connection: JSONRPCEngine,
        for numberClosure: @escaping () throws -> BlockNumber
    ) -> BaseOperation<String> {
        let requestOperation = JSONRPCListOperation<String>(
            engine: connection,
            method: RPCMethod.getBlockHash
        )

        requestOperation.configurationBlock = {
            do {
                let blockNumber = try numberClosure()
                requestOperation.parameters = [blockNumber.toHex()]
            } catch {
                requestOperation.result = .failure(error)
            }
        }

        return requestOperation
    }
}

extension BlockNumber {
    func toHex() -> String {
        var blockNumber = self

        return Data(
            Data(bytes: &blockNumber, count: MemoryLayout<UInt32>.size).reversed()
        ).toHex(includePrefix: true)
    }
}
