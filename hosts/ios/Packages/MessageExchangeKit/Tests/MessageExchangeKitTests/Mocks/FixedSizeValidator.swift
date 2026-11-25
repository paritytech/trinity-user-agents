@testable import MessageExchangeKit

final class FixedSizeValidator: OutgoingRequestSizeValidating {
    let maxPayloadSize: Int

    init(maxPayloadSize: Int) {
        self.maxPayloadSize = maxPayloadSize
    }
}
