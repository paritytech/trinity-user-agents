import Foundation
import Operation_iOS
import SubstrateSdk
import ChainRegistry

@testable import polkadot_app

final class MockRuntimeProvider: RuntimeProviderProtocol {
    var chainId: ChainModel.Id = ""
    var hasSnapshot: Bool = false
    var coderFactory: RuntimeCoderFactoryProtocol?

    func fetchCoderFactoryOperation() -> BaseOperation<RuntimeCoderFactoryProtocol> {
        if let factory = coderFactory {
            return ClosureOperation { factory }
        }
        return ClosureOperation { throw BaseOperationError.unexpectedDependentResult }
    }

    func setup() {}
    func replaceChainData(_: ChainModel) {}
    func cleanup() {}
}
