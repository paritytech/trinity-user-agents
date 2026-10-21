import DesignSystem
import SwiftUI

/// The preview text of a chat while an activity continues, for example "Alice is typing". Three dots after
/// the text pulse in turn. The dots do not move when the user asks for reduced motion.
struct DSChatListOngoingText: View {
    let text: String

    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        HStack(alignment: .center, spacing: DSSpacings.tiny) {
            Text(text)
                .typography(.paragraphMedium)
                .foregroundStyle(Color.fgSecondary)
                .lineLimit(1)
            DSChatListOngoingDots(isAnimated: !reduceMotion)
                .accessibilityHidden(true)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

struct DSChatListOngoingDots: View {
    let isAnimated: Bool

    var body: some View {
        if isAnimated {
            PhaseAnimator(DSChatListOngoingMetrics.phases) { activeDot in
                dots(activeDot: activeDot)
            } animation: { _ in
                .easeInOut(duration: DSChatListOngoingMetrics.stepDuration)
            }
        } else {
            dots(activeDot: nil)
        }
    }

    private func dots(activeDot: Int?) -> some View {
        HStack(spacing: DSChatListOngoingMetrics.spacing) {
            ForEach(DSChatListOngoingMetrics.phases, id: \.self) { index in
                Circle()
                    .fill(Color.fgSecondary)
                    .frame(width: DSChatListOngoingMetrics.dotSize, height: DSChatListOngoingMetrics.dotSize)
                    .opacity(activeDot.map { $0 == index ? 1 : DSChatListOngoingMetrics.dimOpacity } ?? 1)
            }
        }
    }
}

private enum DSChatListOngoingMetrics {
    static let phases = [0, 1, 2]
    static let stepDuration = 0.35
    static let spacing: CGFloat = 2
    static let dotSize: CGFloat = 4
    static let dimOpacity = 0.3
}

#if DEBUG
    #Preview("Ongoing") {
        VStack(alignment: .leading, spacing: 16) {
            DSChatListOngoingText(text: "Alice is typing")
            DSChatListOngoingDots(isAnimated: false)
        }
        .padding(24)
        .background(Color.bgSurfaceMain)
    }
#endif
