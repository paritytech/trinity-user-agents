import Foundation

/// Release thresholds for the Balanced and Max privacy presets. Min privacy releases immediately and takes none.
public struct CoinagePrivacyConfig: Equatable, Sendable {
    public struct Thresholds: Equatable, Sendable {
        public let ringFillPercent: UInt32
        public let anonymityMinutes: UInt32
        public let anonymityMembers: UInt32

        public init(ringFillPercent: UInt32, anonymityMinutes: UInt32, anonymityMembers: UInt32) {
            self.ringFillPercent = ringFillPercent
            self.anonymityMinutes = anonymityMinutes
            self.anonymityMembers = anonymityMembers
        }
    }

    public let balanced: Thresholds
    public let maxPrivacy: Thresholds

    public init(balanced: Thresholds, maxPrivacy: Thresholds) {
        self.balanced = balanced
        self.maxPrivacy = maxPrivacy
    }

    /// Bundled defaults; Remote Config `coinage_privacy_config` overrides them.
    public static let bundled = CoinagePrivacyConfig(
        balanced: .init(ringFillPercent: 20, anonymityMinutes: 10, anonymityMembers: 8),
        maxPrivacy: .init(ringFillPercent: 90, anonymityMinutes: 10, anonymityMembers: 16)
    )
}
