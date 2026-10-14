import DesignSystem
import SwiftUI

public struct DSChatListBadge: View, Equatable {
    public enum Kind: Hashable {
        case reaction
        case counter(Int)
    }

    private let kind: Kind
    private let isMuted: Bool

    public init(kind: Kind, isMuted: Bool = false) {
        self.kind = kind
        self.isMuted = isMuted
    }

    public var body: some View {
        content
    }

    @ViewBuilder
    private var content: some View {
        switch kind {
        case .reaction:
            ZStack {
                Circle()
                    .fill(surfaceColor)
                Image(.icon16HeartSolid)
                    .resizable()
                    .scaledToFit()
                    .foregroundStyle(Color.fgPrimaryInverted)
                    .frame(
                        width: DSChatListBadgeMetrics.reactionIconSize,
                        height: DSChatListBadgeMetrics.reactionIconSize
                    )
            }
            .frame(
                width: DSChatListBadgeMetrics.badgeMinSize,
                height: DSChatListBadgeMetrics.badgeMinSize
            )
        case let .counter(value):
            Text(formatted(value))
                .typography(.labelMedium.emphasized)
                .foregroundStyle(Color.fgPrimaryInverted)
                .padding(.horizontal, DSSpacings.extraSmall)
                .frame(
                    minWidth: DSChatListBadgeMetrics.badgeMinSize,
                    minHeight: DSChatListBadgeMetrics.badgeMinSize
                )
                .background(surfaceColor, in: Capsule())
        }
    }

    private var surfaceColor: Color {
        isMuted ? Color.bgIllustrationDarkMuted : Color.bgIllustrationDark
    }

    private func formatted(_ value: Int) -> String {
        value > 999 ? "999+" : "\(value)"
    }
}

private enum DSChatListBadgeMetrics {
    static let badgeMinSize: CGFloat = 20
    static let reactionIconSize: CGFloat = 16
    static let mutedOpacity: CGFloat = 0.4
}

#if DEBUG
    #Preview("Badges") {
        VStack(spacing: 12) {
            HStack(spacing: 12) {
                DSChatListBadge(kind: .reaction)
                DSChatListBadge(kind: .reaction, isMuted: true)
                DSChatListBadge(kind: .counter(1))
                DSChatListBadge(kind: .counter(42))
                DSChatListBadge(kind: .counter(999))
                DSChatListBadge(kind: .counter(1_500))
            }
            HStack(spacing: 12) {
                DSChatListBadge(kind: .counter(3), isMuted: true)
                DSChatListBadge(kind: .counter(99), isMuted: true)
            }
        }
        .padding(24)
        .background(Color.bgSurfaceMain)
    }
#endif
