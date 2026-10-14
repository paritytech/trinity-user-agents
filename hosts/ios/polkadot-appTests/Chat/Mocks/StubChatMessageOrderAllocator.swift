import Foundation

@testable import polkadot_app

struct StubChatMessageOrderAllocator: ChatMessageOrderAllocating {
    let value: UInt64

    func nextOrder(floor _: () throws -> UInt64) -> UInt64 {
        value
    }
}
