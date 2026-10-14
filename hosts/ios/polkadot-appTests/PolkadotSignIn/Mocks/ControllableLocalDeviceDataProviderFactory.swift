@testable import polkadot_app
import MessageExchangeKit

final class ControllableLocalDeviceDataProviderFactory: LocalDeviceDataProviderMaking {
    private let continuation: AsyncStream<[Chat.LocalDevice]>.Continuation
    private let stream: AsyncStream<[Chat.LocalDevice]>

    init() {
        (stream, continuation) = AsyncStream.makeStream()
    }

    func subscribeDevices() -> AsyncStream<[Chat.LocalDevice]> {
        stream
    }

    func send(_ devices: [Chat.LocalDevice]) {
        continuation.yield(devices)
    }
}
