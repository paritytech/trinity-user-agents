@testable import polkadot_app

final class MockDeviceSyncExchange: DeviceSyncExchanging, @unchecked Sendable {
    private let connection: Int
    private let failureHandler: @Sendable (DeviceSyncConnectionFailure) async -> Void
    private let closeHandler: @Sendable (Int) -> Void

    init(
        connection: Int,
        failureHandler: @escaping @Sendable (DeviceSyncConnectionFailure) async -> Void,
        closeHandler: @escaping @Sendable (Int) -> Void
    ) {
        self.connection = connection
        self.failureHandler = failureHandler
        self.closeHandler = closeHandler
    }

    func start() async {}
    func close() async { closeHandler(connection) }
    func fail(_ failure: DeviceSyncConnectionFailure) async { await failureHandler(failure) }
}
