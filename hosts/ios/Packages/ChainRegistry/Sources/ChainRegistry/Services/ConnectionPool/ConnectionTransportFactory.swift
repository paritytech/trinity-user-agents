import Foundation
import SubstrateSdk
import Starscream

public final class ConnectionTransportFactory: WebSocketConnectionFactoryProtocol {
    private let shouldConnect: ((URL) -> Bool)?

    /// Optional policy evaluated before every physical dial, including SDK
    /// reconnects. A refusal follows the existing engine cancellation path.
    public init(shouldConnect: ((URL) -> Bool)? = nil) {
        self.shouldConnect = shouldConnect
    }

    public func createConnection(
        for url: URL,
        processingQueue: DispatchQueue,
        connectionTimeout: TimeInterval
    ) -> WebSocketConnectionProtocol {
        let request = URLRequest(url: url, timeoutInterval: connectionTimeout)

        let engine = WSEngine(
            transport: TCPTransport(),
            certPinner: FoundationSecurity(),
            compressionHandler: nil
        )

        let connection = WebSocket(request: request, engine: engine)
        connection.callbackQueue = processingQueue

        if let shouldConnect {
            return GuardedConnection(connection: connection, url: url, shouldConnect: shouldConnect)
        }

        return connection
    }
}

private final class GuardedConnection: WebSocketConnectionProtocol {
    private let connection: WebSocket
    private let url: URL
    private let shouldConnect: (URL) -> Bool

    var callbackQueue: DispatchQueue { connection.callbackQueue }
    var delegate: WebSocketDelegate? {
        get { connection.delegate }
        set { connection.delegate = newValue }
    }

    init(connection: WebSocket, url: URL, shouldConnect: @escaping (URL) -> Bool) {
        self.connection = connection
        self.url = url
        self.shouldConnect = shouldConnect
    }

    func connect() {
        guard shouldConnect(url) else {
            connection.didReceive(event: .cancelled)
            return
        }
        connection.connect()
    }

    func disconnect(closeCode: UInt16) {
        connection.disconnect(closeCode: closeCode)
    }

    func forceDisconnect() {
        connection.forceDisconnect()
    }

    func write(string: String, completion: (() -> Void)?) {
        connection.write(string: string, completion: completion)
    }

    func write(stringData: Data, completion: (() -> Void)?) {
        connection.write(stringData: stringData, completion: completion)
    }

    func write(data: Data, completion: (() -> Void)?) {
        connection.write(data: data, completion: completion)
    }

    func write(ping: Data, completion: (() -> Void)?) {
        connection.write(ping: ping, completion: completion)
    }

    func write(pong: Data, completion: (() -> Void)?) {
        connection.write(pong: pong, completion: completion)
    }
}
