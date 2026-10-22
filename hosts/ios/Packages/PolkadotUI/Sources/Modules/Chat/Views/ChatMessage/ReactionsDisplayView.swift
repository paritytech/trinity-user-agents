import DesignSystem
import SwiftUI
import UIKit
import CoreHaptics

internal import SnapKit

public final class ReactionPickerUIView: UIView {
    private let stackView = UIStackView()
    private var onReactionSelected: ((String) -> Void)?
    public var onExpandTapped: (() -> Void)?

    public init(emojis: [String], onReactionSelected: @escaping (String) -> Void) {
        self.onReactionSelected = onReactionSelected
        super.init(frame: .zero)
        setupView(emojis: emojis)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) { fatalError() }
}

// MARK: - Private functions

extension ReactionPickerUIView {
    private func setupView(emojis: [String]) {
        backgroundColor = .bgSurfaceContainer
        layer.cornerRadius = 24

        stackView.axis = .horizontal
        stackView.spacing = 0
        stackView.alignment = .center
        stackView.distribution = .fillEqually

        for emoji in emojis {
            let button = UIButton(type: .system)
            button.setTitle(emoji, for: .normal)
            button.titleLabel?.font = .systemFont(ofSize: 28)
            button.addTarget(self, action: #selector(emojiTapped(_:)), for: .touchUpInside)
            stackView.addArrangedSubview(button)
        }

        let expandButton = makeExpandButton()

        addSubview(stackView)
        addSubview(expandButton)

        expandButton.snp.makeConstraints { make in
            make.trailing.equalToSuperview().inset(8)
            make.centerY.equalToSuperview()
            make.width.height.equalTo(36)
        }

        stackView.snp.makeConstraints { make in
            make.top.bottom.equalToSuperview().inset(8)
            make.leading.equalToSuperview().inset(8)
            make.trailing.equalTo(expandButton.snp.leading).offset(-4)
        }
    }

    private func makeExpandButton() -> UIButton {
        let button = UIButton(type: .custom)
        let config = UIImage.SymbolConfiguration(pointSize: 26, weight: .regular)
        let image = UIImage(systemName: "chevron.down.circle.fill", withConfiguration: config)?
            .withRenderingMode(.alwaysTemplate)
        button.setImage(image, for: .normal)
        button.tintColor = .fgSecondary
        button.imageView?.tintColor = .fgSecondary
        button.addTarget(self, action: #selector(expandTapped), for: .touchUpInside)
        return button
    }

    @objc
    private func emojiTapped(_ sender: UIButton) {
        guard let emoji = sender.title(for: .normal) else { return }
        onReactionSelected?(emoji)
    }

    @objc
    private func expandTapped() {
        onExpandTapped?()
    }
}

public struct ReactionsDisplayView: View, Hashable {
    public let reactions: [ReactionViewModel]
    public let onReactionTap: ((String) -> Void)?
    public let onReactionLongPress: (() -> Void)?

    public init(
        reactions: [ReactionViewModel],
        onReactionTap: ((String) -> Void)? = nil,
        onReactionLongPress: (() -> Void)? = nil
    ) {
        self.reactions = reactions
        self.onReactionTap = onReactionTap
        self.onReactionLongPress = onReactionLongPress
    }

    public var body: some View {
        if !reactions.isEmpty {
            HStack(spacing: DSSpacings.small) {
                ForEach(reactions) { reaction in
                    ReactionBubble(
                        emoji: reaction.emoji,
                        count: reaction.count,
                        isSelected: reaction.isSelectedByCurrentUser,
                        onTap: {
                            onReactionTap?(reaction.emoji)
                        },
                        onLongPress: {
                            onReactionLongPress?()
                        }
                    )
                }
            }
        }
    }

    public static func == (lhs: ReactionsDisplayView, rhs: ReactionsDisplayView) -> Bool {
        lhs.reactions == rhs.reactions
    }

    public func hash(into hasher: inout Hasher) {
        hasher.combine(reactions)
    }
}

private struct ReactionBubble: View {
    let emoji: String
    let count: Int
    let isSelected: Bool
    let onTap: () -> Void
    let onLongPress: () -> Void

    @State private var scale: CGFloat = 0.0
    @State private var hasAppeared = false
    @State private var isPressing = false
    @State private var hapticEngine: CHHapticEngine?
    @State private var hapticPlayer: CHHapticPatternPlayer?

    private let heavyFeedback = UIImpactFeedbackGenerator(style: .heavy)

    var body: some View {
        HStack(spacing: DSSpacings.extraSmall) {
            Text(emoji)
                .typography(.emojiSmall)
                .lineLimit(1)
                .minimumScaleFactor(0.75)

            if count > 1 {
                Text(String(count))
                    .typography(.bodyMedium)
                    .foregroundStyle(Color.fgPrimary)
            }
        }
        .padding(.vertical, DSSpacings.extraSmall)
        .padding(.horizontal, DSSpacings.small)
        .background(backgroundColor, in: Capsule())
        .overlay(
            Capsule().strokeBorder(Color.strokeCutout, lineWidth: 1)
        )
        .scaleEffect(isPressing ? 0.85 : scale)
        .animation(.easeInOut(duration: 0.3), value: isPressing)
        .onAppear {
            withAnimation(.spring(response: 0.4, dampingFraction: 0.6, blendDuration: 0)) {
                scale = 1.0
            }
            hasAppeared = true
        }
        .onChange(of: count) { _, _ in
            if hasAppeared {
                triggerBounceAnimation()
            }
        }
        .onTapGesture {
            handleTap()
        }
        .onLongPressGesture(
            minimumDuration: 0.4,
            pressing: { pressing in
                isPressing = pressing
                if pressing {
                    startGrowingHaptic()
                } else {
                    stopGrowingHaptic()
                }
            },
            perform: {
                stopGrowingHaptic()
                heavyFeedback.impactOccurred()
                onLongPress()
            }
        )
    }
}

// MARK: - Private functions

extension ReactionBubble {
    private func startGrowingHaptic() {
        guard CHHapticEngine.capabilitiesForHardware().supportsHaptics else { return }

        do {
            hapticEngine = try CHHapticEngine()
            try hapticEngine?.start()

            let intensity = CHHapticEventParameter(parameterID: .hapticIntensity, value: 0.3)
            let sharpness = CHHapticEventParameter(parameterID: .hapticSharpness, value: 0.3)

            let continuousEvent = CHHapticEvent(
                eventType: .hapticContinuous,
                parameters: [intensity, sharpness],
                relativeTime: 0,
                duration: 0.5
            )

            let intensityCurve = CHHapticParameterCurve(
                parameterID: .hapticIntensityControl,
                controlPoints: [
                    CHHapticParameterCurve.ControlPoint(relativeTime: 0, value: 0.3),
                    CHHapticParameterCurve.ControlPoint(relativeTime: 0.15, value: 0.5),
                    CHHapticParameterCurve.ControlPoint(relativeTime: 0.3, value: 0.7),
                    CHHapticParameterCurve.ControlPoint(relativeTime: 0.5, value: 1.0)
                ],
                relativeTime: 0
            )

            let sharpnessCurve = CHHapticParameterCurve(
                parameterID: .hapticSharpnessControl,
                controlPoints: [
                    CHHapticParameterCurve.ControlPoint(relativeTime: 0, value: 0.2),
                    CHHapticParameterCurve.ControlPoint(relativeTime: 0.5, value: 0.8)
                ],
                relativeTime: 0
            )

            let pattern = try CHHapticPattern(
                events: [continuousEvent],
                parameterCurves: [intensityCurve, sharpnessCurve]
            )

            hapticPlayer = try hapticEngine?.makePlayer(with: pattern)
            try hapticPlayer?.start(atTime: CHHapticTimeImmediate)
        } catch {}
    }

    private func stopGrowingHaptic() {
        try? hapticPlayer?.stop(atTime: CHHapticTimeImmediate)
        hapticPlayer = nil
        hapticEngine?.stop()
        hapticEngine = nil
    }
}

// MARK: - Private functions

extension ReactionBubble {
    private func handleTap() {
        if !isSelected {
            triggerBounceAnimation()
        }
        onTap()
    }

    private func triggerBounceAnimation() {
        withAnimation(.spring(response: 0.3, dampingFraction: 0.5, blendDuration: 0)) {
            scale = 1.2
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.15) {
            withAnimation(.spring(response: 0.3, dampingFraction: 0.6, blendDuration: 0)) {
                scale = 1.0
            }
        }
    }

    private var backgroundColor: Color {
        isSelected ? Color.bgActionTertiary : Color.bgSurfaceContainer
    }
}

#Preview {
    VStack {
        ReactionsDisplayView(
            reactions: [
                ReactionViewModel(emoji: "👍", count: 5, isSelectedByCurrentUser: true),
                ReactionViewModel(emoji: "❤️", count: 3, isSelectedByCurrentUser: false),
                ReactionViewModel(emoji: "😂", count: 12, isSelectedByCurrentUser: false)
            ]
        )
        .padding()
        .background(Color.black)
        .cornerRadius(16)
    }
    .padding()
}
