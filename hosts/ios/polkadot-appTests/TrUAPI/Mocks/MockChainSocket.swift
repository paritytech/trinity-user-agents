import Foundation
import ChainRegistry

final class MockChainSocket: ChainSocketProtocol, @unchecked Sendable {
    weak var delegate: ChainSocketDelegate?

    private(set) var openCallCount = 0
    private(set) var closeCallCount = 0
    private(set) var sent: [String] = []

    var onOpen: ((MockChainSocket) -> Void)?

    func open() {
        openCallCount += 1
        onOpen?(self)
    }

    func send(text: String) {
        sent.append(text)
    }

    func close() {
        closeCallCount += 1
    }

    func simulateReceive(_ text: String) {
        delegate?.chainSocket(self, didReceive: text)
    }

    func simulateClose(_ reason: ChainSocketCloseReason = .peerClosed) {
        delegate?.chainSocket(self, didCloseWith: reason)
    }
}
