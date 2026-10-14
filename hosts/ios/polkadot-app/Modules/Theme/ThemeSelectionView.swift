import DesignSystem
import ExternalAccessibility
import PolkadotUI
import SwiftUI

struct ThemeSelectionView: View {
    @State private var model: ThemeSelectionViewModel

    @State private var swatchCenters: [ThemeSelection: CGPoint] = [:]
    @State private var containerSize: CGSize = .zero
    @State private var sheetHeight: CGFloat = 0
    @State private var bottomSafeInset: CGFloat = 0
    @State private var revealQueue = RevealQueue()

    @State private var chatAppeared = false
    @State private var chromeAppeared = false
    @State private var swatchesAppeared = false
    @State private var didAnimateEntrance = false

    private static let revealSpace = "themeReveal"
    private static let revealDuration = 0.75

    init(model: ThemeSelectionViewModel) {
        _model = State(initialValue: model)
    }

    var body: some View {
        GeometryReader { proxy in
            ZStack {
                screenContent(titleSelection: model.committed)
                    .environment(\.appTheme, model.committed)

                ForEach(revealQueue.descriptors) { reveal in
                    ThemeRevealLayer(
                        center: reveal.center,
                        startDiameter: ThemeSelectionMetrics.swatchSize,
                        targetDiameter: reveal.targetDiameter,
                        duration: Self.revealDuration,
                        onCovered: {
                            model.commit(reveal.selection)
                            revealQueue.removeUpThrough(id: reveal.id)
                        },
                        content: {
                            screenContent(titleSelection: reveal.selection)
                                .environment(\.appTheme, reveal.selection)
                        }
                    )
                    .allowsHitTesting(false)
                }

                screenContent(swatchOnly: true)
                    .environment(\.appTheme, model.committed)
            }
            .overlay(alignment: .topLeading) {
                if model.showsBackButton {
                    backButton
                        .padding(.leading, DSSpacings.mediumIncreased)
                }
            }
            .frame(width: proxy.size.width, height: proxy.size.height)
            .coordinateSpace(name: Self.revealSpace)
            .onPreferenceChange(SwatchCenterPreferenceKey.self) { swatchCenters = $0 }
            .onChange(of: proxy.size, initial: true) { _, newSize in containerSize = newSize }
            .onChange(of: proxy.safeAreaInsets.bottom, initial: true) { _, inset in bottomSafeInset = inset }
            .sensoryFeedback(.selection, trigger: model.selected)
            .onAppear(perform: animateEntrance)
            .navigationBarBackButtonHidden()
        }
    }
}

// MARK: - Helper

private extension ThemeSelectionView {
    @ViewBuilder
    func screenContent(
        swatchOnly: Bool = false,
        titleSelection: ThemeSelection? = nil
    ) -> some View {
        VStack(spacing: 0) {
            topRegion(
                swatchOnly: swatchOnly,
                titleSelection: titleSelection
            )
            bottomSheet
                .opacity(swatchOnly ? 0 : 1)
                .allowsHitTesting(!swatchOnly)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background {
            if !swatchOnly {
                Color.bgSurfaceMain.ignoresSafeArea()
            }
        }
    }

    var backButton: some View {
        Button {
            model.back()
        } label: {
            Image(uiImage: NavigationBarStyle.backIndicatorImage)
                .renderingMode(.template)
                .foregroundStyle(Color.fgPrimary)
        }
        .buttonStyle(.dsIcon(style: .ghost, shape: .pill, size: .medium, glass: true))
        .environment(\.appTheme, model.committed)
    }

    func animateEntrance() {
        guard !didAnimateEntrance else { return }

        didAnimateEntrance = true
        chatAppeared = true

        guard model.waitsForBubblesBeforeChrome else {
            showChrome()
            return
        }

        let lastBubbleStart = Double(ThemeSelectionEntrance.bubbleCount - 1) * ThemeSelectionEntrance.bubbleStagger
        let bubblesFinish = lastBubbleStart + ThemeSelectionEntrance.bubbleDuration

        Task { @MainActor in
            try await Task.sleep(for: .seconds(bubblesFinish))
            showChrome()
        }
    }

    func showChrome() {
        withAnimation(.spring(response: 0.5, dampingFraction: 0.85)) {
            chromeAppeared = true
        }
        swatchesAppeared = true
    }

    func revealTheme(_ swatch: ThemeSelectionViewModel.Swatch) {
        model.select(swatch.id)

        guard let center = swatchCenters[swatch.id], containerSize.width > 0 else {
            model.commit(swatch.id)
            return
        }

        revealQueue.add(
            selection: swatch.id,
            center: center,
            targetDiameter: revealDiameter(from: center, in: containerSize)
        )
    }

    func revealDiameter(
        from center: CGPoint,
        in size: CGSize
    ) -> CGFloat {
        let horizontalReach = max(center.x, size.width - center.x)
        let verticalReach = max(center.y, size.height - center.y)

        return (hypot(horizontalReach, verticalReach) + 160) * 2
    }
}

// MARK: - Top region

private extension ThemeSelectionView {
    func topRegion(swatchOnly: Bool, titleSelection: ThemeSelection?) -> some View {
        VStack(spacing: DSSpacings.large) {
            Spacer(minLength: 0)
            chatPreview
                .opacity(swatchOnly ? 0 : 1)
                .allowsHitTesting(!swatchOnly)
            Spacer(minLength: 0)
            // Circles/dots draw only in the top (swatchOnly) layer so their tap and selection
            // animations stay above the reveal.
            // The selected title draws in the content layers below the reveal
            // so the expanding theme sweeps over and recolors it.
            swatchSection(
                swatchOnly: swatchOnly,
                titleSelection: titleSelection
            )
            .allowsHitTesting(swatchOnly)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .padding(.bottom, DSSpacings.large)
    }

    var chatPreview: some View {
        VStack(spacing: DSSpacings.large) {
            ForEach(model.previewMessages) { message in
                bubbleRow(message.sender) {
                    DSChatMessageBubble(
                        text: message.text,
                        sender: message.sender,
                        timestamp: message.timestamp,
                        deliveryStatus: message.deliveryStatus,
                        reference: message.reference
                    )
                }
                .modifier(BubbleEntrance(appeared: chatAppeared, index: message.id))
            }
        }
        .frame(maxWidth: .infinity)
        .padding(.horizontal, DSSpacings.large)
    }

    @ViewBuilder
    func bubbleRow(_ sender: DSChatMessageBubble.Sender, @ViewBuilder _ bubble: () -> some View) -> some View {
        HStack(spacing: 0) {
            switch sender {
            case .me:
                Spacer(minLength: ThemeSelectionMetrics.bubbleInset)
                bubble()
            case .other:
                bubble()
                Spacer(minLength: ThemeSelectionMetrics.bubbleInset)
            }
        }
    }

    func swatchSection(
        swatchOnly: Bool,
        titleSelection: ThemeSelection?
    ) -> some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(alignment: .top, spacing: DSSpacings.large) {
                ForEach(model.swatches) { swatch in
                    swatchView(
                        swatch,
                        swatchOnly: swatchOnly,
                        titleSelection: titleSelection
                    )
                    .scaleEffect(swatchesAppeared ? 1 : 0)
                    .animation(
                        ThemeSelectionEntrance
                            .swatchPop
                            .delay(Double(swatch.index) * ThemeSelectionEntrance.swatchStagger),
                        value: swatchesAppeared
                    )
                }
            }
            .padding(.horizontal, DSSpacings.large)
        }
        .scrollClipDisabled()
    }

    func swatchView(
        _ swatch: ThemeSelectionViewModel.Swatch,
        swatchOnly: Bool,
        titleSelection: ThemeSelection?
    ) -> some View {
        let isSelected = model.selected == swatch.id
        return Button {
            revealTheme(swatch)
        } label: {
            VStack(spacing: DSSpacings.small) {
                ZStack {
                    Circle()
                        .fill(swatch.backgroundColor)
                        .frame(width: ThemeSelectionMetrics.swatchSize, height: ThemeSelectionMetrics.swatchSize)
                        .shadow(color: .black.opacity(0.24), radius: 2, x: 0, y: 1)
                    if isSelected {
                        Circle()
                            .fill(swatch.foregroundColor)
                            .frame(
                                width: ThemeSelectionMetrics.swatchDotSize,
                                height: ThemeSelectionMetrics.swatchDotSize
                            )
                            .transition(.scale.combined(with: .opacity))
                    }
                }
                .background(
                    GeometryReader { geometry in
                        Color.clear.preference(
                            key: SwatchCenterPreferenceKey.self,
                            value: [swatch.id: geometry.frame(in: .named(Self.revealSpace)).center]
                        )
                    }
                )
                .opacity(swatchOnly ? 1 : 0)

                Text(swatch.name)
                    .typography(.titleSmall)
                    .foregroundStyle(Color.fgPrimary)
                    .lineLimit(1)
                    .fixedSize()
                    .frame(width: ThemeSelectionMetrics.swatchSize)
                    .opacity(swatch.id == titleSelection ? 1 : 0)
            }
        }
        .buttonStyle(SwatchButtonStyle())
    }
}

// MARK: - Bottom sheet

private extension ThemeSelectionView {
    var bottomSheet: some View {
        VStack {
            VStack(alignment: .leading, spacing: DSSpacings.small) {
                Text(model.title)
                    .typography(.headlineLarge)
                    .foregroundStyle(Color.fgPrimary)
                Text(model.subtitle)
                    .typography(.paragraphLarge)
                    .foregroundStyle(Color.fgSecondary)
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, DSSpacings.large)
            .padding(.bottom, DSSpacings.mediumIncreased)

            if model.showsConfirmButton {
                DSButton(
                    model.confirmTitle,
                    style: .primary,
                    shape: .pill,
                    size: .large,
                    expands: true
                ) {
                    model.confirm()
                }
                .padding(.horizontal, DSSpacings.mediumIncreased)
                .accessibilityId(AccessibilityID.Onboarding.themeContinueButton)
            }
        }
        .geometryGroup()
        .padding(.top, DSSpacings.large)
        .padding(.bottom, DSSpacings.large)
        .frame(maxWidth: .infinity)
        .background(alignment: .top) {
            UnevenRoundedRectangle(
                topLeadingRadius: DSRadii.extraLarge,
                bottomLeadingRadius: 0,
                bottomTrailingRadius: 0,
                topTrailingRadius: DSRadii.extraLarge,
                style: .continuous
            )
            .fill(Color.bgSurfaceContainer)
            .ignoresSafeArea(edges: .bottom)
        }
        .background {
            GeometryReader { geometry in
                Color.clear.onChange(of: geometry.size.height, initial: true) { _, height in
                    sheetHeight = height
                }
            }
        }
        .offset(y: chromeAppeared ? 0 : hiddenSheetOffset)
    }

    var hiddenSheetOffset: CGFloat {
        (
            sheetHeight > 0
                ? sheetHeight
                : ThemeSelectionMetrics.sheetOffsetFloor
        ) + bottomSafeInset
    }
}

#if DEBUG
    #Preview("Theme selection") {
        ThemeSelectionView(model: ThemeSelectionViewModel(themeManager: ThemeManager.shared) {})
    }

    #Preview("Theme selection (settings)") {
        ThemeSelectionView(
            model: ThemeSelectionViewModel(
                themeManager: ThemeManager.shared,
                context: .settings
            ) {}
        )
    }
#endif
