import Foundation
import IssueMonitoring
import UIKit

/// One step of a product's game reminder, from booking the alarm to opening the product. Each step names
/// the product and the game start, so a missed game can be followed through the host.
struct GameReminderEvent: Equatable {
    enum Step: String {
        case scheduled
        case notScheduled = "not_scheduled"
        case fired
        case tapped
        case productOpened = "product_opened"
        case cancelled
    }

    enum Delivery: String {
        case alarm
        case notification
    }

    enum SkipReason: String {
        case notAuthorized = "not_authorized"
        case startInPast = "start_in_past"
        case failed
    }

    enum OpenSource: String {
        case autoOpen = "auto_open"
        case alarm
    }

    enum AppState: String {
        case active
        case inactive
        case background

        init?(_ state: UIApplication.State) {
            switch state {
            case .active:
                self = .active
            case .inactive:
                self = .inactive
            case .background:
                self = .background
            @unknown default:
                return nil
            }
        }
    }

    let step: Step
    let productId: String
    /// Nil when the alarm's stored start is no longer available.
    let startsAt: Date?
    let occurredAt: Date
    let delivery: Delivery?
    let skipReason: SkipReason?
    let openSource: OpenSource?
    let appState: AppState?

    init(
        step: Step,
        productId: String,
        startsAt: Date?,
        occurredAt: Date = Date(),
        delivery: Delivery? = nil,
        skipReason: SkipReason? = nil,
        openSource: OpenSource? = nil,
        appState: AppState? = nil
    ) {
        self.step = step
        self.productId = productId
        self.startsAt = startsAt
        self.occurredAt = occurredAt
        self.delivery = delivery
        self.skipReason = skipReason
        self.openSource = openSource
        self.appState = appState
    }

    /// Times are device wall clock. The alarm rings at the start, so `delay_after_start_ms` is also how
    /// late a step came against the alarm's fire time.
    func hostEvent(origin: HostEventOrigin, monotonicMilliseconds: Int) -> HostEvent {
        var tags = [
            "host.os_version": origin.osVersion,
            "reminder.product_id": productId
        ]
        tags["reminder.delivery"] = delivery?.rawValue
        tags["reminder.skip_reason"] = skipReason?.rawValue
        tags["reminder.open_source"] = openSource?.rawValue
        tags["host.app_state"] = appState?.rawValue

        var extras = ["app.launch_id": HostEventValue.string(origin.launchID.uuidString.lowercased())]
        if let startsAt {
            extras["reminder.starts_at_unix_ms"] = .integer(Int(startsAt.timeIntervalSince1970 * 1_000))
            extras["reminder.delay_after_start_ms"] = .integer(Int(occurredAt.timeIntervalSince(startsAt) * 1_000))
        }

        return .make(
            name: "host.reminder.\(step.rawValue)",
            origin: origin,
            monotonicMilliseconds: monotonicMilliseconds,
            tags: tags,
            extras: extras
        )
    }
}
