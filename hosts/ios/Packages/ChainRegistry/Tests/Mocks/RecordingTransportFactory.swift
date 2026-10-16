import Foundation
import os
import SubstrateSdk

final class RecordingTransportFactory: WebSocketConnectionFactoryProtocol, @unchecked Sendable {
    private let connections = OSAllocatedUnfairLock<[(url: URL, connection: RecordingWebSocketConnection)]>(
        uncheckedState: []
    )

    var created: [(url: URL, connection: RecordingWebSocketConnection)] {
        connections.withLockUnchecked { $0 }
    }

    func createConnection(
        for url: URL,
        processingQueue: DispatchQueue,
        connectionTimeout _: TimeInterval
    ) -> WebSocketConnectionProtocol {
        let connection = RecordingWebSocketConnection(callbackQueue: processingQueue)
        connections.withLockUnchecked { $0.append((url, connection)) }
        return connection
    }
}
