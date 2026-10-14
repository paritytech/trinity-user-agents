@testable import polkadot_app
import Foundation
import MessageExchangeKit

struct EmptyLocalDeviceDataProviderFactory: LocalDeviceDataProviderMaking {
    func subscribeDevices() -> AsyncStream<[Chat.LocalDevice]> {
        AsyncStream { $0.finish() }
    }
}
