import Foundation
import os
@testable import polkadot_app

/// Records the open and close calls of a room screen. `failsToOpen` makes each open throw.
final class RecordingChatRoomLifecycle: ChatRoomLifecycle {
    enum Call: Equatable {
        case open(String)
        case close(String)
    }

    struct OpenFailure: Error {}

    private let failsToOpen: Bool
    private let recorded = OSAllocatedUnfairLock<[Call]>(initialState: [])

    init(failsToOpen: Bool = false) {
        self.failsToOpen = failsToOpen
    }

    var calls: [Call] {
        recorded.withLock { $0 }
    }

    func open(roomId: String) async throws {
        recorded.withLock { $0.append(.open(roomId)) }

        if failsToOpen {
            throw OpenFailure()
        }
    }

    func close(roomId: String) async {
        recorded.withLock { $0.append(.close(roomId)) }
    }
}
