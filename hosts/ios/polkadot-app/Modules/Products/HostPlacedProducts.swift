import Foundation
import KeyDerivation

/// A product the host itself puts in chat: present on first run, with no add, install or discover
/// step, and removable by nobody. The chat analogue of Pocket's privileged card
/// (`docs/rfcs/pocket-modality.md`), shaped like its `AssetPinnedPocketCards`.
///
/// Placing one grants no permissions; it still prompts like any other product.
struct HostPlacedProduct: Sendable {
    /// Must match the room the product registers, which iOS gives the host no way to check. A
    /// product that registers another room, or none, gets that chat as well and leaves this one
    /// empty beside it. Pocket's `Definition` names its `cardId` the same way, but owns card ids
    /// in a way no host owns a room id.
    let roomId: String

    /// Shown until the published manifest resolves and supplies the real name.
    let fallbackName: String

    /// Reserved identity as a function of the network TLD, so one entry covers every chain.
    let productId: @Sendable (String) -> String

    /// Placed only while ``SettingsManagerProtocol/isHostPlacementEnabled`` is on. True for a
    /// product a native extension duplicates on `FEATURE_DIMS` builds (Jollity and WeeklyGame);
    /// a product with no native twin is placed on every build.
    let followsPlacementSwitch: Bool
}

enum HostPlacedProducts {
    /// Which of these this build places is ``placed(enabled:)``.
    static let all: [HostPlacedProduct] = [
        HostPlacedProduct(
            roomId: "jollity",
            fallbackName: "Jollity",
            productId: BuiltInProduct.dim2(for:),
            followsPlacementSwitch: true
        ),
        HostPlacedProduct(
            roomId: "humanity",
            fallbackName: "Humanity",
            productId: BuiltInProduct.personhood(for:),
            followsPlacementSwitch: false
        )
    ]

    /// The products placed under the current ``SettingsManagerProtocol/isHostPlacementEnabled``.
    static func placed(enabled: Bool) -> [HostPlacedProduct] {
        all.filter { enabled || !$0.followsPlacementSwitch }
    }

    /// Read against the candidate's own TLD, so it answers without the chain read that listing
    /// needs — the trick Pocket's `pinned(key:)` uses. An exact match, not a prefix test, so
    /// `app.dim2.dot` is a different publisher and is not host-placed.
    static func contains(productId: String) -> Bool {
        guard let tld = productId.components(separatedBy: ".").last, !tld.isEmpty else {
            return false
        }

        return all.contains { $0.productId(tld) == productId }
    }
}
