import SwiftUI

/// The card as an opened screen draws it: the frame the collection gives it,
/// with the same margins the collection leaves around it.
struct PocketOpenedCardView: View {
    static let verticalInset: CGFloat = 16

    static var height: CGFloat { PocketCardSize.height + verticalInset * 2 }

    let card: PocketCardViewModel
    /// Whether a finger holds a pan on the face, from the start of its drag until it comes to rest.
    var onUserMovingChange: ((Bool) -> Void)?

    var body: some View {
        PocketCollectionCardView(card: card, onUserMovingChange: onUserMovingChange)
            // Only the opened card lets a pan follow the finger: the collection scrolls under the same drag.
            .widgetPansEnabled(true)
            .padding(.horizontal, PocketCardSize.sideMargin)
            .padding(.vertical, Self.verticalInset)
    }
}
