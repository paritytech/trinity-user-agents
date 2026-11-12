import SwiftUI

/// The card as an opened screen draws it: the frame the collection gives it,
/// with the same margins the collection leaves around it.
struct PocketOpenedCardView: View {
    static let verticalInset: CGFloat = 16

    static var height: CGFloat { PocketCardSize.height + verticalInset * 2 }

    let card: PocketCardViewModel

    var body: some View {
        PocketCollectionCardView(card: card)
            .padding(.horizontal, PocketCardSize.sideMargin)
            .padding(.vertical, Self.verticalInset)
    }
}
