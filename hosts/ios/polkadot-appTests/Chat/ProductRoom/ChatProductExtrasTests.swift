import Foundation
import PolkadotUI
import Testing
@testable import polkadot_app

/// The live parts of the footer and the placeholder of a product room (P6.8, scenario S37).
@MainActor
struct ChatProductExtrasTests {
    @Test func aChangeOfTheAccessoryReachesItsObservers() {
        let model = ChatRoomAccessoryModel()
        var changes = 0
        let observation = model.observe { changes += 1 }

        model.readAloudStopTitle = "Stop"
        model.readAloudStopTitle = "Stop"
        model.readAloudStopTitle = nil

        #expect(changes == 2)
        _ = observation
    }

    // MARK: - Placeholder (S37)

    @Test func thePlaceholderWaitsForTheWholeRoom() {
        var model = MessageListModel.stub([])
        #expect(model.showsPlaceholder)

        model.paging = ChatPagingState(older: .loading, newer: .end)
        #expect(!model.showsPlaceholder)

        model.paging = .complete
        model.readiness = .loading
        #expect(!model.showsPlaceholder)
    }
}
