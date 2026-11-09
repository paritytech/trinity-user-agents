import SwiftUI

/// Header with details underneath while expanded. The host scrolls it. `overscroll` is the host's rubber-band
/// distance past the top, which the header cancels out so a pull stretches only the details.
public struct DSExpandableCardLayout<Card: View, Details: View>: View {
    private let isExpanded: Bool
    private let overscroll: CGFloat
    private let card: () -> Card
    private let details: () -> Details

    public init(
        isExpanded: Bool,
        overscroll: CGFloat = 0,
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
            card()
                .offset(y: isExpanded ? -overscroll : 0)
                .zIndex(1)

            if isExpanded {
                details()
            }
        }
        .frame(maxWidth: .infinity, alignment: .top)
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
