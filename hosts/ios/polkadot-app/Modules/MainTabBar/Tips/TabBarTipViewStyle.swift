import PolkadotUI
import SwiftUI
import TipKit

/// Replaces TipKit's automatic body so the popover has no close button — the configuration
/// exposes no such element, so it simply is not drawn. Tapping the body stands in for it:
/// `.tipClosed` is what advances `TabBarTipOrderedSequence` to the next step.
struct TabBarTipViewStyle: TipViewStyle {
    func makeBody(configuration: Configuration) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            configuration.title
                .font(.headline)
            configuration.message
                .font(.subheadline)
        }
        .foregroundStyle(Color.fgPrimaryInverted)
        .padding()
        .contentShape(Rectangle())
        .onTapGesture {
            configuration.tip.invalidate(reason: .tipClosed)
        }
    }
}
