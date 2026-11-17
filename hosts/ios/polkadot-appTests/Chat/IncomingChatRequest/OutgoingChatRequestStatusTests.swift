@testable import polkadot_app
import Testing

struct OutgoingChatRequestStatusTests {
    @Test func cancelledRawValueRoundTrips() {
        #expect(Chat.RequestStatus(rawValue: 7) == .outgoing(.cancelled))
        #expect(Chat.RequestStatus.outgoing(.cancelled).rawValue == 7)
        #expect(Chat.RequestStatus.outgoing(.pending).rawValue == 5)
    }
}
