import CryptoKit
import Foundation
import Keystore_iOS
import Testing
import TrUAPIHost
import UserNotifications
@testable import polkadot_app

/// A product room gets a push route (a push id and a key) that the notification service extension reads, and a
/// product push opens with that key into a notification that opens the room.
struct AppPushHostBridgeTests {
    private let token = Data(repeating: 0xAB, count: 32)

    @Test func aDeliveryGoesToTheRelayAsTheNativeChatSendsIt() async throws {
        let relay = RecordingRelay()
        let bridge = AppPushHostBridge(deviceToken: { nil }, relay: relay)

        let taken = try await bridge.sendPush(
            productId: "verbosity.dot",
            delivery: HostPushDelivery(
                service: .apns,
                token: "abcd",
                pushId: String(repeating: "1", count: 32),
                message: "00ff",
                voip: false
            )
        )

        #expect(taken)
        let sent = try #require(relay.sent.first)
        #expect(sent.deviceToken == "abcd")
        #expect(sent.platform == "ios")
        #expect(sent.message == "00ff")
        #expect(!sent.voip)
    }

    /// A call offer from the chat product has to reach the contact as a VoIP push, which iOS
    /// hands to CallKit at once; any other push must not, or iOS stops waking the app for VoIP.
    @Test func aChatPushKeepsItsVoIPFlag() async throws {
        let relay = RecordingRelay()
        let bridge = AppPushHostBridge(deviceToken: { nil }, relay: relay)

        _ = try await bridge.sendPush(
            productId: "chat.paseo",
            delivery: HostPushDelivery(
                service: .apns,
                token: "abcd",
                pushId: String(repeating: "2", count: 64),
                message: "00ff",
                voip: true
            )
        )

        let sent = try #require(relay.sent.first)
        #expect(sent.voip)
        #expect(sent.pushId == String(repeating: "2", count: 64))
    }

    @Test func pushTokensAreTheHexOfTheAPNsAndVoIPTokens() async throws {
        let bridge = AppPushHostBridge(
            deviceToken: { Data([0xA1, 0xB2]) },
            voipToken: { Data([0xC3]) },
            relay: RecordingRelay()
        )

        let tokens = try await bridge.pushTokens()

        #expect(tokens == HostPushTokens(service: .apns, token: "a1b2", voipToken: "c3"))
    }
}

private final class RecordingRelay: ProductPushRelaying, @unchecked Sendable {
    private(set) var sent: [NotifyRequestParameters] = []

    func notify(_ parameters: NotifyRequestParameters) async throws -> Bool {
        sent.append(parameters)
        return true
    }
}
