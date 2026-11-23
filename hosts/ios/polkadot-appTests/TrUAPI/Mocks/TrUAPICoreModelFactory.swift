import Foundation
@testable import polkadot_app

@MainActor
func makeExecutionModel(
    execution: MockProductExecution = MockProductExecution(),
    chainConnections: MockChainConnections = MockChainConnections()
) -> RustRuntimeEnvironment.ExecutionModel {
    let osPermissionAsker = MockOSPermissionAsker()
    let bridge = RustProductExecutionBridge(dependencies: makeChatBridgeDependencies(
        chainConnections: chainConnections,
        osPermissionAsker: osPermissionAsker
    ))
    bridge.attach(execution)
    return RustRuntimeEnvironment.ExecutionModel(
        execution: execution,
        chainConnections: chainConnections,
        media: bridge.media,
        osPermissionAsker: osPermissionAsker,
        bridge: bridge
    )
}
