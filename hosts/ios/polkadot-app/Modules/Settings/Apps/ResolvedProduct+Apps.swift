import Foundation
import PolkadotUI
import Products

/// Shared presentation of a product across the apps list and the app detail header.
extension ResolvedProduct {
    /// Nil for legacy products, which are named after their domain — a subtitle would repeat it.
    var domainSubtitle: String? {
        displayName == id ? nil : id
    }

    /// Drawn under the icon until it loads. Keyed on the domain rather than the display name so it
    /// matches the letter the icon loader falls back to, which knows only the domain — otherwise a
    /// product with no icon would visibly swap letters once that fallback arrived.
    var placeholderAvatar: AvatarViewModel {
        .colored(text: String(id.prefix(1)), colorSeed: id)
    }
}
