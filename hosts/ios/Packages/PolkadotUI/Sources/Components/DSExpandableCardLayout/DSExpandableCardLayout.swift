import Observation
import SwiftUI

/// Header with details underneath while expanded. The host scrolls it. `overscroll` reports the host's rubber-band
/// distance past the top, which the header cancels out so a pull stretches only the details.
public struct DSExpandableCardLayout<Card: View, Details: View>: View {
    private let isExpanded: Bool
    private let overscroll: DSOverscroll?
    private let card: () -> Card
    private let details: () -> Details

    public init(
        isExpanded: Bool,
        overscroll: DSOverscroll? = nil,
        @ViewBuilder card: @escaping () -> Card,
        @ViewBuilder details: @escaping () -> Details
    ) {
        self.isExpanded = isExpanded
        self.overscroll = overscroll
        self.card = card
        self.details = details
    }

    public var body: some View {
        VStack(spacing: 16) {
            PinnedHeader(overscroll: overscroll, isExpanded: isExpanded, card: card)

            if isExpanded {
                details()
            }
        }
        .frame(maxWidth: .infinity, alignment: .top)
    }
}

/// The host scroll view's rubber-band distance past the top, which an expanded card's header cancels out.
/// Only the header reads it, so a pull redraws the header alone.
@MainActor
@Observable
public final class DSOverscroll {
    public var distance: CGFloat = 0

    public init() {}
}

/// The header, offset against the rubber band while expanded. Reads `overscroll` in its own body so a pull does not
/// redraw the details.
private struct PinnedHeader<Card: View>: View {
    let overscroll: DSOverscroll?
    let isExpanded: Bool
    let card: () -> Card

    var body: some View {
        card()
            .offset(y: isExpanded ? -(overscroll?.distance ?? 0) : 0)
            .zIndex(1)
    }
}

public enum DSCardCollapse {
    public static func distance(in containerHeight: CGFloat) -> CGFloat {
        if #available(iOS 26, *) {
            max(containerHeight / Constants.fractionOnGlass, Constants.minimumDistance)
        } else {
            max(containerHeight / Constants.fraction, Constants.minimumDistance)
        }
    }
}

private extension DSCardCollapse {
    enum Constants {
        static let fraction: CGFloat = 6
        static let fractionOnGlass: CGFloat = 4
        static let minimumDistance: CGFloat = 1
    }
}
