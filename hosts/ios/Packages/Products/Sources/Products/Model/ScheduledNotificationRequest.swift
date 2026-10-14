import Foundation

public struct ScheduledNotificationRequest: Decodable {
    public let text: String
    public let deeplink: String?
    public let scheduledAtMs: UInt64?

    public init(text: String, deeplink: String?, scheduledAtMs: UInt64?) {
        self.text = text
        self.deeplink = deeplink
        self.scheduledAtMs = scheduledAtMs
    }
}
