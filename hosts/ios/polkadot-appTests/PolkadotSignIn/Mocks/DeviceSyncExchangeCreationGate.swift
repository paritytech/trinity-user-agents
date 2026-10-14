actor DeviceSyncExchangeCreationGate {
    private let blockedConnection: Int
    private let stream: AsyncStream<Void>
    private let continuation: AsyncStream<Void>.Continuation
    private var isOpen = false

    init(blockedConnection: Int = 1) {
        self.blockedConnection = blockedConnection
        (stream, continuation) = AsyncStream.makeStream(bufferingPolicy: .bufferingNewest(1))
    }

    func wait(connection: Int) async {
        guard connection == blockedConnection else { return }
        guard !isOpen else { return }
        for await _ in stream {
            return
        }
    }

    func open() {
        guard !isOpen else { return }
        isOpen = true
        continuation.yield(())
        continuation.finish()
    }
}
