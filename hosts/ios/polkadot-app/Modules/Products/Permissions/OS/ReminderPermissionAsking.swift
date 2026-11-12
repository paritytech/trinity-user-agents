import Foundation

/// OS authorizations a product's game reminder is delivered through. Each ask prompts only when
/// the user was never asked, and otherwise answers the standing decision.
protocol ReminderPermissionAsking: Sendable {
    /// Whether an AlarmKit alarm may ring; always false below iOS 26.1.
    func askAlarm() async -> Bool

    func askNotifications() async -> Bool
}
