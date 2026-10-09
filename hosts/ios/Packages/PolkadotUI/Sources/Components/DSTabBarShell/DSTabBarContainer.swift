import DesignSystem
import SwiftUI

/// Notched tab bar with four caller-supplied slots. The first two sit left of a centre gap the
/// width of the hole, the last two sit right of it. The bar supplies chrome and layout only;
/// each slot draws itself.
public struct DSTabBarContainer<First: View, Second: View, Third: View, Fourth: View>: View {
    private let first: First
    private let second: Second
    private let third: Third
    private let fourth: Fourth

    /// Slots are placed left to right, two either side of the hole, each taking an equal share
    /// of the width left over.
    public init(
        @ViewBuilder first: () -> First,
        @ViewBuilder second: () -> Second,
        @ViewBuilder third: () -> Third,
        @ViewBuilder fourth: () -> Fourth
    ) {
        self.first = first()
        self.second = second()
        self.third = third()
        self.fourth = fourth()
    }

    public var body: some View {
        HStack(spacing: 0) {
            first.frame(maxWidth: .infinity)
            second.frame(maxWidth: .infinity)
            Color.clear.frame(width: DSTabBarStyle.notchRadius * 2)
            third.frame(maxWidth: .infinity)
            fourth.frame(maxWidth: .infinity)
        }
        .padding(.horizontal, DSTabBarStyle.itemInset)
        .frame(height: DSTabBarStyle.height)
        .background(DSTabBarMaterial())
        .clipShape(DSTabBarShape(), style: FillStyle(eoFill: true))
        .overlay(border)
        .shadow(color: Color.shadowSoft, radius: DSTabBarStyle.shadowRadius, y: DSTabBarStyle.shadowRadius / 3)
        .mask { DSTabBarShadowMask().fill(style: FillStyle(eoFill: true)) }
        .background(widthReader)
    }
}

private extension DSTabBarContainer {
    /// Strokes the outline only. The hole is left unstroked because clipping its edge leaves
    /// a hairline around it instead of a clean edge. The stroke is clipped back to the bar,
    /// which keeps only the inner half of every line, so its width is doubled first.
    ///
    /// The rim runs on the same axis as the material's wash, so it lightens the bar on dark
    /// themes and darkens it on light ones, brightest at the top and faded by the foot.
    var border: some View {
        RoundedRectangle(cornerRadius: DSTabBarStyle.cornerRadius, style: .continuous)
            .stroke(
                LinearGradient(
                    colors: [
                        Color.fgPrimary.opacity(DSTabBarStyle.rimTopOpacity),
                        Color.fgPrimary.opacity(DSTabBarStyle.rimBottomOpacity)
                    ],
                    startPoint: .top,
                    endPoint: .bottom
                ),
                lineWidth: DSTabBarStyle.strokeWidth * 2
            )
            .clipShape(DSTabBarShape(), style: FillStyle(eoFill: true))
    }

    var widthReader: some View {
        GeometryReader { proxy in
            Color.clear.preference(key: DSTabBarWidthKey.self, value: proxy.size.width)
        }
    }
}

/// Publishes the bar's width up the view tree, so a centre control can size itself to it.
struct DSTabBarWidthKey: PreferenceKey {
    static let defaultValue: CGFloat = 0

    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) {
        value = max(value, nextValue())
    }
}

#if DEBUG
    private struct DSTabBarPreviewBar: View {
        var body: some View {
            DSTabBarContainer(
                first: { item("house.fill") },
                second: { item("bubble.left.fill") },
                third: { item("safari.fill") },
                fourth: { item("gearshape.fill") }
            )
            .padding(.horizontal, 16)
            .padding(.top, 30)
            .padding(.bottom, 16)
            .background(Color.bgSurfaceMain)
        }

        private func item(_ symbol: String) -> some View {
            Image(systemName: symbol)
                .font(.system(size: 17, weight: .semibold))
                .foregroundStyle(Color.fgPrimary)
        }
    }

    #Preview("Berlin Night (dark)") {
        DSTabBarPreviewBar()
            .environment(\.appTheme, .berlinNight)
    }

    #Preview("Berlin Day (light)") {
        DSTabBarPreviewBar()
            .environment(\.appTheme, .berlinDay)
    }
#endif
