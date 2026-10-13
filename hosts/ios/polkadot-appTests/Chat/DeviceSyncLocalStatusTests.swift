import Testing
@testable import polkadot_app

struct DeviceSyncLocalStatusTests {
    @Test func failedOutgoingSyncsAsPendingForDevicesWithoutFailedStatus() {
        #expect(Chat.DeviceSyncLocalStatus(from: .outgoing(.failed)) == .outgoing(.new))
    }

    @Test func failedOutgoingStatusPersistsAsRawFive() {
        #expect(Chat.LocalMessage.Status.outgoing(.failed).rawValue == 5)
        #expect(Chat.LocalMessage.Status(rawValue: 5) == .outgoing(.failed))
    }
}
