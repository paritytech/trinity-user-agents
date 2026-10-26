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
    private let selectedSlot: Int?
    private let isBarHidden: Bool
    private let playsBarHiddenFeedback: Bool
    private let openProgress: CGFloat
    private let first: First
    private let second: Second
    private let third: Third
    private let fourth: Fourth
    private let center: Center
    private let shelf: Shelf

    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Namespace private var selectionNamespace
    /// Bumped on every slot tap to play the selection tick.
    @State private var slotTapCount = 0

    /// Slots are placed left to right, two either side of the hole, each taking an equal share
    /// of the width left over. The centre slot is laid over the bar after its mask, so a control
    /// in the hole can grow above the bar without being clipped. `availableHeight` is the height
    /// the shelf may take above the bar row; the container does not measure the screen. The shelf content
    /// must have an intrinsic height because the shelf is sized from it. `shelfContentHeight` receives the
    /// shelf content's measured height, so the caller can size its touch region to the shelf. `selectedSlot`
    /// is the index, 0 to 3 left to right, of the slot the selection pill sits behind. `nil` draws no bar pill,
    /// which is also the case while a shelf view holds the pill through `dsTabBarSelectionPill`.
    /// `isBarHidden` fades the bar out, shelf included, leaving the centre slot. Changing it while the centre panel
    /// is fully open plays a medium impact unless `playsBarHiddenFeedback` is false. `openProgress` is the centre
    /// control's open progress, 0 closed to 1 open; the bar row blurs, fades and rises with it. A tap on any slot
    /// plays a selection tick.
    public init(
        isShelfOpen: Binding<Bool>,
        shelfContentHeight: Binding<CGFloat>,
        availableHeight: CGFloat,
        selectedSlot: Int?,
        isBarHidden: Bool,
        playsBarHiddenFeedback: Bool = true,
        openProgress: CGFloat,
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
        self.selectedSlot = selectedSlot
        self.isBarHidden = isBarHidden
        self.playsBarHiddenFeedback = playsBarHiddenFeedback
        self.openProgress = openProgress
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
                slot(first, at: 0)
                slot(second, at: 1)
                Color.clear.frame(width: DSTabBarStyle.notchShoulderWidth)
                slot(third, at: 2)
                slot(fourth, at: 3)
            }
            .padding(.horizontal, DSTabBarStyle.itemInset)
            .frame(height: DSTabBarStyle.height)
            .accessibilityElement(children: .contain)
            .accessibilityAddTraits(.isTabBar)
            .animation(selectionAnimation, value: selectedSlot)
            .sensoryFeedback(.selection, trigger: slotTapCount)
            .blur(radius: DSTabBarStyle.openItemBlurRadius * openProgress)
            .opacity(1 - (1 - DSTabBarStyle.openItemOpacity) * openProgress)
        }
        .environment(\.dsTabBarSelectionNamespace, selectionNamespace)
        .background(DSTabBarBarMaterial())
        .clipShape(DSTabBarShape(), style: FillStyle(eoFill: true))
        .overlay(border)
        .offset(y: -DSTabBarStyle.openBarRise * openProgress)
        .opacity(isBarHidden ? 0 : 1)
        .allowsHitTesting(!isBarHidden)
        .accessibilityHidden(isBarHidden)
        .animation(barFadeAnimation, value: isBarHidden)
        .sensoryFeedback(trigger: isBarHidden) { _, _ in
            playsBarHiddenFeedback && openProgress == 1 ? .impact(weight: .medium) : nil
        }
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
    func slot(_ content: some View, at index: Int) -> some View {
        content
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .simultaneousGesture(TapGesture().onEnded { slotTapCount += 1 })
            .background {
                if selectedSlot == index {
                    selectionPill
                }
            }
    }

    var selectionPill: some View {
        DSTabBarSelectionPill.fill(Capsule(style: .continuous))
            .padding(.vertical, DSTabBarStyle.selectionInset)
            .frame(width: DSTabBarStyle.selectionWidth)
            .matchedGeometryEffect(id: DSTabBarSelectionPill.matchedGeometryId, in: selectionNamespace)
    }

    /// Eased in every mode: a fade has no travel for Reduce Motion to remove.
    var barFadeAnimation: Animation {
        .easeInOut(duration: DSTabBarStyle.shelfFadeDuration)
    }

    /// Snaps under Reduce Motion so the pill jumps between slots without the travel.
    var selectionAnimation: Animation? {
        guard !reduceMotion else { return nil }
        return DSTabBarStyle.selectionCurve
    }

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
                // Clears the hole, which reaches up into the shelf's bottom edge.
                .padding(.bottom, DSTabBarStyle.notchOverhang)
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

    /// Strokes the outline of the bar. The stroke is clipped back to the bar, which keeps only the inner half of
    /// every line, so its width is doubled first. The notch edge is stroked too.
    var border: some View {
        DSTabBarShape()
            .stroke(DSTabBarStyle.Palette.strokeGlass, lineWidth: DSTabBarStyle.strokeWidth * 2)
            .clipShape(DSTabBarShape(), style: FillStyle(eoFill: true))
    }
}

#if DEBUG
    private struct DSTabBarPreviewBar: View {
        @State private var shelfRowCount: Int
        @State private var isShelfOpen: Bool
        @State private var shelfContentHeight: CGFloat = 0
        @State private var selectedSlot = 0
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
                    selectedSlot: selectedSlot,
                    isBarHidden: false,
                    openProgress: 0,
                    first: { item("house.fill", at: 0) },
                    second: { item("bubble.left.fill", at: 1) },
                    third: { item("safari.fill", at: 2) },
                    fourth: { item("gearshape.fill", at: 3) },
                    center: { EmptyView() },
                    shelf: { shelfRows }
                )
            }
            .padding(.horizontal, 16)
            .padding(.top, 30)
            .padding(.bottom, 16)
            .background(Color.bgSurfaceMain)
        }

        private func item(_ symbol: String, at index: Int) -> some View {
            Button {
                selectedSlot = index
            } label: {
                Image(systemName: symbol)
                    .font(.system(size: 17, weight: .semibold))
                    .foregroundStyle(Color.fgPrimary)
            }
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
