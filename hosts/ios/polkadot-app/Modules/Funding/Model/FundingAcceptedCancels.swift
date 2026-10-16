import Foundation

/// The sessions whose cancel offer the user accepted. A payout that lands
/// after the acceptance ends the session as sent, and the core then forgets
/// the cancel, so the app keeps its own note to tell the user the cancel came
/// too late.
enum FundingAcceptedCancels {
    private static let key = "funding.acceptedCancels"
    private static let limit = 64

    /// Notes that the user accepted the cancel offer on session `intent`.
    static func remember(_ intent: String, in defaults: UserDefaults = .standard) {
        var intents = defaults.stringArray(forKey: key) ?? []
        guard !intents.contains(intent) else { return }
        intents.append(intent)
        defaults.set(Array(intents.suffix(limit)), forKey: key)
    }

    /// Whether the user accepted a cancel offer on session `intent`.
    static func contains(_ intent: String, in defaults: UserDefaults = .standard) -> Bool {
        defaults.stringArray(forKey: key)?.contains(intent) ?? false
    }
}
