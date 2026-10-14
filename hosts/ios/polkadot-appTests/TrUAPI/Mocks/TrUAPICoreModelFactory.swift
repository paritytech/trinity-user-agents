import Foundation
@testable import polkadot_app

func makeExecutionModel(
    execution: MockProductExecution = MockProductExecution(),
    chainConnections: MockChainConnections = MockChainConnections(),
    osPermissionAsker: MockOSPermissionAsker = MockOSPermissionAsker()
) -> RustRuntimeEnvironment.ExecutionModel {
    RustRuntimeEnvironment.ExecutionModel(
        execution: execution,
        chainConnections: chainConnections,
        osPermissionAsker: osPermissionAsker
    )
}
