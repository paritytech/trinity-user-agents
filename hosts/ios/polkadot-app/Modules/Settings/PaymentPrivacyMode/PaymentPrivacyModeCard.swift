import SwiftUI
import UIKit
import PolkadotUI
import DesignSystem
import Coinage

/// Inline "Payments Privacy Mode" row — the first cell of the Security & Privacy group (the enclosing
/// layout supplies the grouped-cell surface). Three modes as lit spheres resting in a recessed groove that
/// carries a stepped speed-to-privacy scale, and a ring marking the selection. Tap a mode and the ring
/// slides to it; drag the ring and it follows the finger, settling on the nearest mode on release. The
/// spheres stay in their slots and grow or shrink as the ring comes and goes. A description card follows.
///
/// A dumb view: it renders the ``selected`` mode supplied by the Settings view model and reports user
/// input through ``onSelect``; the presenter/interactor own persistence and re-gating.
struct PaymentPrivacyModeCard: View {
    let selected: RecyclingStrategyType
    let onSelect: (RecyclingStrategyType) -> Void

    private let modes = RecyclingStrategyType.allCases

    @State private var ringPosition: CGFloat
    @State private var isDragging = false
    @State private var ringTravelling = false
    @State private var pendingIndex: Int?
    @State private var journey = 0
    @State private var markIndex = 0
    @State private var dragFadeDuration = PrivacyModeMetrics.slowDragFade
    @State private var lastDragX: CGFloat?
    @State private var lastDragTime: Date?

    init(selected: RecyclingStrategyType, onSelect: @escaping (RecyclingStrategyType) -> Void) {
        self.selected = selected
        self.onSelect = onSelect
        _ringPosition = State(initialValue: CGFloat(RecyclingStrategyType.allCases.firstIndex(of: selected) ?? 0))
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header

            VStack(spacing: DSSpacings.small) {
                selector
                descriptionCard
            }
        }
        .padding(.horizontal, DSSpacings.mediumIncreased)
        .padding(.bottom, DSSpacings.small)
        .sensoryFeedback(.selection, trigger: selected)
        .sensoryFeedback(.selection, trigger: markIndex)
        .onChange(of: selected) { _, _ in selectionChanged() }
    }
}

// MARK: - Header

private extension PaymentPrivacyModeCard {
    var header: some View {
        HStack(spacing: DSSpacings.smallIncreased) {
            Image(systemName: "shield")
                .font(.system(size: 18, weight: .regular))
                .foregroundStyle(.fgSecondary)
                .frame(width: 32, height: 32)

            Text(String(localized: .settingsPrivacymodeTitle(PaymentAssetBranding.shared.current.symbol)))
                .typography(.bodyLarge)
                .foregroundStyle(.fgPrimary)
        }
        .padding(.vertical, DSSpacings.small)
        .frame(minHeight: PrivacyModeMetrics.headerMinHeight)
    }
}

// MARK: - Selector

private extension PaymentPrivacyModeCard {
    var selector: some View {
        GeometryReader { geo in
            let width = geo.size.width
            ZStack {
                trackGroove(width: width)
                TickScale(start: centerX(0, width: width), end: centerX(lastIndex, width: width))
                circles(width: width)
                ring(width: width)
            }
            .frame(width: width, height: PrivacyModeMetrics.boxHeight)
            .contentShape(Rectangle())
            .gesture(dragGesture(width: width))
        }
        .frame(height: PrivacyModeMetrics.boxHeight)
    }

    func trackGroove(width: CGFloat) -> some View {
        // The floor is the card's own surface taken a shade down — a recess, not a darker token that would
        // read as a hole punched through the card. Depth comes from the two inner shadows below.
        let floor = Color.bgSurfaceContainer.blended(with: .bgSurfaceMain, fraction: 0.4)
        // The shadow tokens are the same black in every theme, so on the light themes' near-white floor a
        // fixed alpha reads as a bruise; soften it by the floor's luminance (untouched on the dark floor).
        let recess = Color.shadowMedium.opacity(0.5).softened(on: floor)
        // Two copies of the groove a point above and below it, covered by the opaque floor: what stays
        // visible is the lit lip along the top and bottom edges only, never a full outline.
        return ZStack {
            Capsule().fill(Color.bgSurfaceNested).offset(y: -1)
            Capsule().fill(Color.bgSurfaceNested).offset(y: 1)
            Capsule()
                .fill(
                    floor
                        .shadow(.inner(color: recess, radius: 6, y: 3))
                        .shadow(.inner(color: recess, radius: 5, y: 7))
                )
        }
        .frame(width: width, height: PrivacyModeMetrics.trackHeight)
    }

    func circles(width: CGFloat) -> some View {
        ForEach(Array(modes.enumerated()), id: \.element) { index, mode in
            ModeCircleView(mode: mode, state: circleState(at: index))
                .position(x: centerX(CGFloat(index), width: width), y: PrivacyModeMetrics.boxHeight / 2)
        }
    }

    func ring(width: CGFloat) -> some View {
        SelectionRingView(
            mode: modes[nearestIndex],
            fadeDuration: isDragging ? dragFadeDuration : PrivacyModeMetrics.tapFade
        )
        .position(x: centerX(ringPosition, width: width), y: PrivacyModeMetrics.boxHeight / 2)
    }

    func circleState(at index: Int) -> ModeCircleState {
        if !isDragging, !ringTravelling, index == chosenIndex { return .settled }
        if index == highlightedIndex { return .grown }
        return .resting
    }
}

// MARK: - Description

private extension PaymentPrivacyModeCard {
    var descriptionCard: some View {
        let shown = modes[highlightedIndex]
        return VStack(alignment: .leading, spacing: DSSpacings.extraTiny) {
            Text(shown.displayTitle)
                .typography(.titleMedium)
                .foregroundStyle(.fgPrimary)

            Text(shown.displayDescription)
                .typography(.paragraphMedium)
                .foregroundStyle(.fgSecondary)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, DSSpacings.mediumIncreased)
        .padding(.vertical, DSSpacings.extraMedium)
        .background(.bgSurfaceNested, in: RoundedRectangle(cornerRadius: DSRadii.extraMedium, style: .continuous))
        .animation(.easeOut(duration: 0.18), value: highlightedIndex)
    }
}

// MARK: - Geometry & interaction

private extension PaymentPrivacyModeCard {
    var selectedIndex: Int { modes.firstIndex(of: selected) ?? 0 }
    var lastIndex: CGFloat { CGFloat(modes.count - 1) }

    /// The mode the ring is closest to. It spends most of a drag between two.
    var nearestIndex: Int {
        min(max(Int(ringPosition.rounded()), 0), modes.count - 1)
    }

    var chosenIndex: Int { pendingIndex ?? selectedIndex }

    var highlightedIndex: Int {
        isDragging ? nearestIndex : chosenIndex
    }

    func centerX(_ position: CGFloat, width: CGFloat) -> CGFloat {
        PrivacyModeMetrics.inset + position * trackStep(width: width)
    }

    /// Distance between neighbouring mode centres.
    func trackStep(width: CGFloat) -> CGFloat {
        guard modes.count > 1 else { return 0 }
        return (width - PrivacyModeMetrics.inset * 2) / lastIndex
    }

    /// A single gesture that reads a tap and a drag apart: a touch that never crosses the slop selects the
    /// mode it lands on (the ring then slides there on its own), while one that does takes the ring along
    /// under the finger and settles it on the nearest mode on release.
    func dragGesture(width: CGFloat) -> some Gesture {
        DragGesture(minimumDistance: 0)
            .onChanged { value in
                guard isDragging || abs(value.translation.width) >= 8 else { return }
                if !isDragging { beginDrag() }
                trackSpeed(locationX: value.location.x, time: value.time, width: width)

                let fraction = fractionAt(value.location.x, width: width)
                ringPosition = fraction

                let scaleStart = centerX(0, width: width)
                markIndex = max(0, Int((centerX(fraction, width: width) - scaleStart) / PrivacyModeMetrics.tickStep))
            }
            .onEnded { value in
                lastDragX = nil
                lastDragTime = nil

                if isDragging {
                    finishDrag()
                } else {
                    commit(target: Int(fractionAt(value.location.x, width: width).rounded()))
                }
            }
    }

    func finishDrag() {
        let target = nearestIndex
        commit(target: target)

        withAnimation(PrivacyModeMetrics.selectionAnimation) {
            ringPosition = CGFloat(target)
        } completion: {
            isDragging = false
        }
    }

    func commit(target: Int) {
        guard target != chosenIndex else { return }
        pendingIndex = target
        onSelect(modes[target])
        if !isDragging { moveRing(to: target) }
    }

    func selectionChanged() {
        pendingIndex = nil
        if !isDragging, ringPosition != CGFloat(selectedIndex) {
            moveRing(to: selectedIndex)
        }
    }

    func moveRing(to target: Int) {
        journey += 1
        let thisJourney = journey
        ringTravelling = true

        withAnimation(PrivacyModeMetrics.selectionAnimation) {
            ringPosition = CGFloat(target)
        } completion: {
            guard thisJourney == journey else { return }
            ringTravelling = false
        }
    }

    func beginDrag() {
        isDragging = true
        dragFadeDuration = PrivacyModeMetrics.slowDragFade
        lastDragX = nil
        lastDragTime = nil
    }

    /// Follows the finger's speed in mode-widths per second and eases the cross-fade duration towards its
    /// fast end, smoothed so a single jittery sample does not swing it.
    func trackSpeed(locationX: CGFloat, time: Date, width: CGFloat) {
        defer {
            lastDragX = locationX
            lastDragTime = time
        }
        guard let previousX = lastDragX, let previousTime = lastDragTime else { return }
        let elapsed = time.timeIntervalSince(previousTime)
        let step = trackStep(width: width)
        guard elapsed > 0, step > 0 else { return }

        let speed = abs(locationX - previousX) / step / CGFloat(elapsed)
        let reach = min(max(speed / PrivacyModeMetrics.fastDragSpeed, 0), 1)
        let target = PrivacyModeMetrics
            .slowDragFade + (PrivacyModeMetrics.fastDragFade - PrivacyModeMetrics.slowDragFade) * Double(reach)
        dragFadeDuration += (target - dragFadeDuration) * 0.4
    }

    /// The fractional mode index at a touch x, clamped to the outer modes.
    func fractionAt(_ locationX: CGFloat, width: CGFloat) -> CGFloat {
        let step = trackStep(width: width)
        guard step > 0 else { return 0 }
        return min(max((locationX - PrivacyModeMetrics.inset) / step, 0), lastIndex)
    }
}

// MARK: - Recess shadow shading

private extension Color {
    /// Scales a shadow's alpha down as the `surface` it lands on brightens, so a fixed black shadow (the
    /// design system's `shadow.*` tokens are the same in every theme) does not read as a bruise on the light
    /// themes. Dynamic: the surface luminance is read from the render-time traits. Sole consumer is
    /// ``trackGroove(width:)``.
    func softened(on surface: Color, falloff: CGFloat = PrivacyModeMetrics.lightSurfaceFalloff) -> Color {
        let shadow = UIColor(self)
        let base = UIColor(surface)

        return Color(uiColor: UIColor { traits in
            let resolvedShadow = shadow.resolvedColor(with: traits)
            let luminance = base.resolvedColor(with: traits).relativeLuminance

            var red: CGFloat = 0, green: CGFloat = 0, blue: CGFloat = 0, alpha: CGFloat = 0
            resolvedShadow.getRed(&red, green: &green, blue: &blue, alpha: &alpha)

            return UIColor(red: red, green: green, blue: blue, alpha: alpha * (1 - luminance * falloff))
        })
    }
}

private extension UIColor {
    /// WCAG relative luminance (0 = black … 1 = white).
    var relativeLuminance: CGFloat {
        var red: CGFloat = 0, green: CGFloat = 0, blue: CGFloat = 0, alpha: CGFloat = 0
        getRed(&red, green: &green, blue: &blue, alpha: &alpha)

        func linear(_ component: CGFloat) -> CGFloat {
            component <= 0.03928 ? component / 12.92 : pow((component + 0.055) / 1.055, 2.4)
        }

        return 0.2126 * linear(red) + 0.7152 * linear(green) + 0.0722 * linear(blue)
    }
}

#if DEBUG
    #Preview("PaymentPrivacyModeCard") {
        StatefulPreviewWrapper(RecyclingStrategyType.balanced) { binding in
            PaymentPrivacyModeCard(selected: binding.wrappedValue) { binding.wrappedValue = $0 }
                .dsMenuListCellSurface(position: .first, showsDivider: true)
                .padding()
                .background(Color.bgSurfaceMain)
        }
    }

    private struct StatefulPreviewWrapper<Value, Content: View>: View {
        @State private var value: Value
        private let content: (Binding<Value>) -> Content

        init(_ initial: Value, @ViewBuilder content: @escaping (Binding<Value>) -> Content) {
            _value = State(initialValue: initial)
            self.content = content
        }

        var body: some View { content($value) }
    }
#endif
