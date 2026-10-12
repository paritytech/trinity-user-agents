import DesignSystem
import SwiftUI

/// Notched tab bar with four caller-supplied slots and a centre slot. The first two sit left of a
/// centre gap the width of the hole, the last two sit right of it. The bar supplies chrome and
/// layout only; each slot draws itself. A shelf of caller-supplied content grows upward from the
/// bar row while open, up to `availableHeight`, and scrolls beyond that. The container draws the
/// shelf and animates its own content-height changes; the caller animates opening and closing by
/// writing `isShelfOpen` inside `withAnimation`.
public struct DSTabBarContainer<
    First: View,
    Second: View,
    Third: View,
    Fourth: View,
    Center: View,
    Shelf: View
>: View {
    @Binding private var isShelfOpen: Bool
    @Binding private var shelfContentHeight: CGFloat
    private let availableHeight: CGFloat
    private let first: First
    private let second: Second
    private let third: Third
    private let fourth: Fourth
    private let center: Center
    private let shelf: Shelf

    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    /// Slots are placed left to right, two either side of the hole, each taking an equal share
    /// of the width left over. The centre slot is laid over the bar after its mask, so a control
    /// in the hole can grow above the bar without being clipped. `availableHeight` is the height
    /// the shelf may take above the bar row; the container does not measure the screen. The shelf content
    /// must have an intrinsic height because the shelf is sized from it. `shelfContentHeight` receives the
    /// shelf content's measured height, so the caller can size its touch region to the shelf.
    public init(
        isShelfOpen: Binding<Bool>,
        shelfContentHeight: Binding<CGFloat>,
        availableHeight: CGFloat,
        @ViewBuilder first: () -> First,
        @ViewBuilder second: () -> Second,
        @ViewBuilder third: () -> Third,
        @ViewBuilder fourth: () -> Fourth,
        @ViewBuilder center: () -> Center,
        @ViewBuilder shelf: () -> Shelf
    ) {
        _isShelfOpen = isShelfOpen
        _shelfContentHeight = shelfContentHeight
        self.availableHeight = availableHeight
        self.first = first()
        self.second = second()
        self.third = third()
        self.fourth = fourth()
        self.center = center()
        self.shelf = shelf()
    }

    public var body: some View {
        VStack(spacing: 0) {
            shelfPanel
            HStack(spacing: 0) {
                first.frame(maxWidth: .infinity)
                second.frame(maxWidth: .infinity)
                Color.clear.frame(width: DSTabBarStyle.notchRadius * 2)
                third.frame(maxWidth: .infinity)
                fourth.frame(maxWidth: .infinity)
            }
            .padding(.horizontal, DSTabBarStyle.itemInset)
            .frame(height: DSTabBarStyle.height)
        }
        .background(DSTabBarMaterial())
        .clipShape(DSTabBarShape(), style: FillStyle(eoFill: true))
        .overlay(border)
        .shadow(color: Color.shadowSoft, radius: DSTabBarStyle.shadowRadius, y: DSTabBarStyle.shadowRadius / 3)
        .mask { DSTabBarShadowMask().fill(style: FillStyle(eoFill: true)) }
        .overlay(alignment: .bottom) { center }
    }
}

/// Carries the shelf content's measured height so the shelf can stop growing at the cap.
private struct DSTabBarShelfHeightKey: PreferenceKey {
    static let defaultValue: CGFloat = 0

    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) {
        value = max(value, nextValue())
    }
}

private extension DSTabBarContainer {
    /// Eased rather than sprung under Reduce Motion, so the shelf reveals without the overshoot
    /// that makes the growth read as motion.
    var shelfAnimation: Animation {
        guard !reduceMotion else { return .easeInOut(duration: DSTabBarStyle.shelfFadeDuration) }
        return .spring(
            response: DSTabBarStyle.shelfSpringResponse,
            dampingFraction: DSTabBarStyle.shelfSpringDamping
        )
    }

    /// The scroll view takes the content's height until it reaches the cap, then stays there and
    /// scrolls. Measuring the content is what keeps a short shelf short: a scroll view offered a
    /// height is otherwise greedy and would always fill the cap.
    var shelfPanel: some View {
        ScrollView(.vertical) {
            shelf
                .background(
                    GeometryReader { proxy in
                        Color.clear.preference(key: DSTabBarShelfHeightKey.self, value: proxy.size.height)
                    }
                )
        }
        .scrollBounceBehavior(.basedOnSize)
        .frame(height: isShelfOpen ? min(shelfContentHeight, availableHeight) : 0)
        .clipped()
        // The measurement arrives in a later update than the open, so without the animation here the height
        // change falls outside the isShelfOpen transaction and the shelf snaps.
        .onPreferenceChange(DSTabBarShelfHeightKey.self) { newHeight in
            withAnimation(shelfAnimation) {
                shelfContentHeight = newHeight
            }
        }
        .opacity(isShelfOpen ? 1 : 0)
        .allowsHitTesting(isShelfOpen)
        .accessibilityHidden(!isShelfOpen)
    }

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
}

#if DEBUG
    private struct DSTabBarPreviewBar: View {
        @State private var shelfRowCount: Int
        @State private var isShelfOpen: Bool
        @State private var shelfContentHeight: CGFloat = 0
        let showsRowControls: Bool

        init(shelfRowCount: Int = 0, showsRowControls: Bool = false) {
            _shelfRowCount = State(initialValue: shelfRowCount)
            _isShelfOpen = State(initialValue: shelfRowCount > 0)
            self.showsRowControls = showsRowControls
        }

        var body: some View {
            VStack {
                if showsRowControls {
                    HStack(spacing: 16) {
                        Button("Fewer") { shelfRowCount = max(shelfRowCount - 1, 0) }
                        Button("More") { shelfRowCount += 1 }
                        Button(isShelfOpen ? "Close" : "Open") {
                            withAnimation(
                                .spring(
                                    response: DSTabBarStyle.shelfSpringResponse,
                                    dampingFraction: DSTabBarStyle.shelfSpringDamping
                                )
                            ) {
                                isShelfOpen.toggle()
                            }
                        }
                    }
                    .foregroundStyle(Color.fgPrimary)
                }
                DSTabBarContainer(
                    isShelfOpen: $isShelfOpen,
                    shelfContentHeight: $shelfContentHeight,
                    availableHeight: 320,
                    first: { item("house.fill") },
                    second: { item("bubble.left.fill") },
                    third: { item("safari.fill") },
                    fourth: { item("gearshape.fill") },
                    center: { EmptyView() },
                    shelf: { shelfRows }
                )
            }
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

        private var shelfRows: some View {
            VStack(spacing: 12) {
                ForEach(0 ..< shelfRowCount, id: \.self) { index in
                    Text("Row \(index + 1)")
                        .font(.system(size: 15, weight: .medium))
                        .foregroundStyle(Color.fgPrimary)
                }
            }
            .frame(maxWidth: .infinity)
            .padding(.vertical, 16)
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

    #Preview("Shelf short (dark)") {
        DSTabBarPreviewBar(shelfRowCount: 3)
            .environment(\.appTheme, .berlinNight)
    }

    #Preview("Shelf tall, scrolls (dark)") {
        DSTabBarPreviewBar(shelfRowCount: 40)
            .environment(\.appTheme, .berlinNight)
    }

    #Preview("Shelf changes height (dark)") {
        DSTabBarPreviewBar(shelfRowCount: 3, showsRowControls: true)
            .environment(\.appTheme, .berlinNight)
    }
#endif
