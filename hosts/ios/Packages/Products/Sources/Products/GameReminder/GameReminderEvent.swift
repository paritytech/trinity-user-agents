import Foundation
import IssueMonitoring

/// One step of a product's game reminder, from booking the alarm to opening the product. Each step names
/// the product and the game start, so a missed game can be followed through the host.
public struct GameReminderEvent: Sendable, Equatable {
    public enum Step: String, Sendable {
        case scheduled
        case notScheduled = "not_scheduled"
        case fired
        case tapped
        case productOpened = "product_opened"
        case missed
        case cancelled
    }

    public enum Delivery: String, Sendable {
        case alarm
        case notification
    }

    public enum SkipReason: String, Sendable {
        case notAuthorized = "not_authorized"
        case startInPast = "start_in_past"
        case failed
    }

    public enum OpenSource: String, Sendable {
        case autoOpen = "auto_open"
        case alarm
    }

    public enum AppState: String, Sendable {
        case active
        case inactive
        case background
    }

    public let step: Step
    public let productId: String
    /// Nil only for an alarm booked before the start was attached to it.
    public let startsAt: Date?
    public let occurredAt: Date
    public var delivery: Delivery?
    public var skipReason: SkipReason?
    public var openSource: OpenSource?
    public var appState: AppState?
    /// False when opening found the product already on screen.
    public var navigated: Bool?

    public init(step: Step, productId: String, startsAt: Date?, occurredAt: Date = Date()) {
        self.step = step
        self.productId = productId
        self.startsAt = startsAt
        self.occurredAt = occurredAt
    }

    public var name: String {
        "host.reminder.\(step.rawValue)"
    }

    /// Times are device wall clock. The alarm rings at the start, so `delay_after_start_ms` is also how
    /// late a step came against the alarm's fire time.
    public func hostEvent(origin: HostEventOrigin) -> HostEvent {
        var tags = [
            "host.app_variant": origin.appVariant,
            "host.platform": "ios",
            "host.os_version": origin.osVersion,
            "reminder.product_id": productId
        ]
        tags["host.native_app_version"] = origin.appVersion
        tags["reminder.delivery"] = delivery?.rawValue
        tags["reminder.skip_reason"] = skipReason?.rawValue
        tags["reminder.open_source"] = openSource?.rawValue
        tags["host.app_state"] = appState?.rawValue

        var extras: [String: HostEventValue] = [
            "app.launch_id": .string(origin.launchID.uuidString.lowercased())
        ]
        extras["host.app_build"] = origin.appBuild.map(HostEventValue.integer)
        extras["reminder.navigated"] = navigated.map(HostEventValue.boolean)
        if let startsAt {
            extras["reminder.starts_at_unix_ms"] = .integer(Int(startsAt.timeIntervalSince1970 * 1_000))
            extras["reminder.delay_after_start_ms"] = .integer(Int(occurredAt.timeIntervalSince(startsAt) * 1_000))
        }

        return HostEvent(name: name, tags: tags, extras: extras)
    }
}
