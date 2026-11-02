import CoreGraphics

/// How a card in the Pocket is drawn while one of its cards is open. The
/// opened card is moved, never hidden, so it keeps its place in the stack: the
/// cards below it stay in front of it. When the Pocket clears for it, as on
/// Android, the cards below slide off the bottom and the cards above shrink back.
struct PocketCardMotion: Equatable {
    let offset: CGSize
    let scale: CGFloat
    let opacity: Double

    static let resting = PocketCardMotion(offset: .zero, scale: 1, opacity: 1)
    static let shrunkAway = PocketCardMotion(offset: .zero, scale: 0.9, opacity: 0)
    /// Android's distance. Four card heights carry even the card just below the
    /// opened one past the bottom of the screen on every iPhone.
    static let slidAway = PocketCardMotion(
        offset: CGSize(width: 0, height: PocketCardSize.height * 4),
        scale: 1,
        opacity: 0
    )

    static func of(cardAt index: Int, openAt openIndex: Int?, openOffset: CGSize, cleared: Bool) -> PocketCardMotion {
        guard let openIndex else { return .resting }

        if index == openIndex {
            return PocketCardMotion(offset: openOffset, scale: 1, opacity: 1)
        }

        guard cleared else { return .resting }

        return index > openIndex ? .slidAway : .shrunkAway
    }
}
