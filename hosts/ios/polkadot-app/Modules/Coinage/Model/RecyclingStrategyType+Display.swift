import SwiftUI
import DesignSystem
import Coinage

/// Presentation for the payment-privacy modes. Colours map to semantic design-system tokens
/// (warning / success / amethyst) rather than raw values.
extension RecyclingStrategyType {
    var displayTitle: String {
        switch self {
        case .minPrivacy: String(localized: .settingsPrivacymodeFastestTitle)
        case .balanced: String(localized: .settingsPrivacymodeBalancedTitle)
        case .maxPrivacy: String(localized: .settingsPrivacymodePrivateTitle)
        }
    }

    var displayDescription: String {
        switch self {
        case .minPrivacy: String(localized: .settingsPrivacymodeFastestDetails)
        case .balanced: String(localized: .settingsPrivacymodeBalancedDetails)
        case .maxPrivacy: String(localized: .settingsPrivacymodePrivateDetails)
        }
    }

    var displayIconName: String {
        switch self {
        case .minPrivacy: "bolt.circle.fill"
        case .balanced: "shield.lefthalf.filled"
        case .maxPrivacy: "eye.slash.fill"
        }
    }

    /// The vivid hue: sphere shading, tick gradient stop, selection ring and glow. Amethyst's vivid value
    /// is the avatar *background* token (`#7C3AED`); the foreground one is a near-white lavender that
    /// washes out to grey when shaded, so it is not the accent.
    var displayAccentColor: Color {
        switch self {
        case .minPrivacy: .fgWarning
        case .balanced: .fgSuccess
        case .maxPrivacy: .avatarBgAmethyst
        }
    }
}
