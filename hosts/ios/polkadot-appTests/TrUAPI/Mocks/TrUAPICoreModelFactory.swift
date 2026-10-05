import Foundation
@testable import polkadot_app

func makeExecutionModel(
    execution: MockProductExecution = MockProductExecution(),
    chainConnections: MockChainConnections = MockChainConnections()
) -> RustRuntimeEnvironment.ExecutionModel {
    RustRuntimeEnvironment.ExecutionModel(
        execution: execution,
        chainConnections: chainConnections,
        media: NativeMediaBackend(productId: "test.dot")
    )
}
