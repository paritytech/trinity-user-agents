import Foundation
@testable import polkadot_app

@MainActor
func makeExecutionModel(
    execution: MockProductExecution = MockProductExecution(),
    chainConnections: MockChainConnections = MockChainConnections(),
    osPermissionAsker: MockOSPermissionAsker = MockOSPermissionAsker()
) -> RustRuntimeEnvironment.ExecutionModel {
    let bridge = RustProductExecutionBridge(dependencies: makeChatBridgeDependencies())
    bridge.attach(execution)
    return RustRuntimeEnvironment.ExecutionModel(
        execution: execution,
        chainConnections: chainConnections,
        osPermissionAsker: osPermissionAsker,
        bridge: bridge
    )
}
