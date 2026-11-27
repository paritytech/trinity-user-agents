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

    /// `clearance` runs from 0, the Pocket at rest, to 1, cleared for the open item.
    static func of(
        cardAt index: Int,
        openAt openIndex: Int?,
        openOffset: CGSize,
        clearance: CGFloat
    ) -> PocketCardMotion {
        guard let openIndex else { return resting.mixed(with: .shrunkAway, by: clearance) }

        if index == openIndex {
            return PocketCardMotion(offset: openOffset, scale: 1, opacity: 1)
        }

        return resting.mixed(with: index > openIndex ? .slidAway : .shrunkAway, by: clearance)
    }
}

private extension PocketCardMotion {
    func mixed(with cleared: PocketCardMotion, by clearance: CGFloat) -> PocketCardMotion {
        let kept = 1 - clearance

        return PocketCardMotion(
            offset: CGSize(
                width: offset.width * kept + cleared.offset.width * clearance,
                height: offset.height * kept + cleared.offset.height * clearance
            ),
            scale: scale * kept + cleared.scale * clearance,
            opacity: opacity * Double(kept) + cleared.opacity * Double(clearance)
        )
    }
}
