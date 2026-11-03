import PolkadotUI
import SwiftUI
import TipKit

/// Replaces TipKit's automatic body so the popover has no close button — the configuration
/// exposes no such element, so it simply is not drawn. Tapping the body stands in for it:
/// `.tipClosed` is what advances `TabBarTipOrderedSequence` to the next step.
///
/// With `onTap` set, the tip is the Docklet's: the body runs `onTap` and invalidates as used, and a
/// separate close button invalidates as closed without running it.
struct TabBarTipViewStyle: TipViewStyle {
    var onTap: (@MainActor () -> Void)?

    @ViewBuilder
    func makeBody(configuration: Configuration) -> some View {
        if let onTap {
            ZStack(alignment: .topTrailing) {
                Button {
                    onTap()
                    configuration.tip.invalidate(reason: .actionPerformed)
                } label: {
                    // 44 pt close target less the 16 pt default padding keeps the title clear of the close button.
                    text(configuration)
                        .padding(.trailing, 28)
                }
                .buttonStyle(.plain)

                Button {
                    configuration.tip.invalidate(reason: .tipClosed)
                } label: {
                    Image(systemName: "xmark")
                        .font(.caption.weight(.bold))
                        .foregroundStyle(Color.fgPrimaryInverted)
                        .frame(width: 44, height: 44)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel(String(localized: .Common.close))
            }
        } else {
            text(configuration)
                .contentShape(Rectangle())
                .onTapGesture {
                    configuration.tip.invalidate(reason: .tipClosed)
                }
        }
    }
}

private extension TabBarTipViewStyle {
    func text(_ configuration: Configuration) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            configuration.title
                .font(.headline)
            configuration.message
                .font(.subheadline)
        }
        .foregroundStyle(Color.fgPrimaryInverted)
        .padding()
    }
}
