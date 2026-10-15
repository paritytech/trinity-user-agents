import DesignSystem
import SwiftUI

/// The chip under a message that has a thread (Chat Extension v2 additions, threads): up to three small avatars of
/// the participants that replied, the number of replies and the time of the last reply. A tap opens the thread.
public struct ChatThreadChipView: View, Hashable {
    public let avatars: [AvatarViewModel]
    /// For example "3 replies · 10:42".
    public let text: String
    public let accessibilityText: String
    public let side: ChatBubbleTailSide
    /// It takes no part in the equality.
    public let onTap: () -> Void

    public init(
        avatars: [AvatarViewModel],
        text: String,
        accessibilityText: String,
        side: ChatBubbleTailSide,
        onTap: @escaping () -> Void
    ) {
        self.avatars = avatars
        self.text = text
        self.accessibilityText = accessibilityText
        self.side = side
        self.onTap = onTap
    }

    public var body: some View {
        HStack(spacing: 0) {
            if side == .trailing { Spacer(minLength: 0) }

            Button(action: onTap) {
                HStack(spacing: DSSpacings.extraSmall) {
                    HStack(spacing: -6) {
                        ForEach(Array(avatars.prefix(3).enumerated()), id: \.offset) { _, avatar in
                            AvatarViewSUI(viewModel: avatar, size: 18)
                                .overlay(Circle().stroke(Color(uiColor: .bgSurfaceContainer), lineWidth: 1))
                        }
                    }

                    Text(text)
                        .typography(.labelSmall)
                        .foregroundStyle(Color(uiColor: .fgPrimary))
                        .lineLimit(1)

                    Image(systemName: "chevron.right")
                        .font(.system(size: 10, weight: .semibold))
                        .foregroundStyle(Color(uiColor: .fgSecondary))
                }
                .padding(.horizontal, DSSpacings.small)
                .padding(.vertical, DSSpacings.extraSmall)
                .background(Capsule().fill(Color(uiColor: .bgSurfaceContainer)))
            }
            .buttonStyle(.plain)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(accessibilityText)
            .accessibilityAddTraits(.isButton)

            if side == .leading { Spacer(minLength: 0) }
        }
        .padding(.top, DSSpacings.extraTiny)
    }

    public static func == (lhs: Self, rhs: Self) -> Bool {
        lhs.avatars == rhs.avatars && lhs.text == rhs.text && lhs.accessibilityText == rhs.accessibilityText
            && lhs.side == rhs.side
    }

    public func hash(into hasher: inout Hasher) {
        hasher.combine(avatars)
        hasher.combine(text)
        hasher.combine(accessibilityText)
        hasher.combine(side)
    }
}

/// The control under a message that has alternatives: "‹ 2/3 ›". An arrow asks the product for another
/// alternative. While the room waits for the answer, the arrows do not respond and a spinner shows.
public struct ChatBranchControlView: View, Hashable {
    public let index: Int
    public let count: Int
    /// The alternative that the room asked for and waits for, or nil.
    public let pendingIndex: Int?
    public let side: ChatBubbleTailSide
    public let previousLabel: String
    public let nextLabel: String
    /// For example "Alternative 2 of 3".
    public let positionLabel: String
    /// Takes the index of the alternative that the user picked. It takes no part in the equality.
    public let onSelect: (Int) -> Void

    public init(
        index: Int,
        count: Int,
        pendingIndex: Int?,
        side: ChatBubbleTailSide,
        labels: Labels,
        onSelect: @escaping (Int) -> Void
    ) {
        self.index = index
        self.count = count
        self.pendingIndex = pendingIndex
        self.side = side
        previousLabel = labels.previous
        nextLabel = labels.next
        positionLabel = labels.position
        self.onSelect = onSelect
    }

    /// The VoiceOver labels of the arrows and of the position.
    public struct Labels {
        public let previous: String
        public let next: String
        public let position: String

        public init(previous: String, next: String, position: String) {
            self.previous = previous
            self.next = next
            self.position = position
        }
    }

    public var body: some View {
        HStack(spacing: DSSpacings.extraSmall) {
            if side == .trailing { Spacer(minLength: 0) }

            arrow("chevron.left", target: index - 1, label: previousLabel)

            Text("\(index)/\(count)")
                .typography(.labelSmall)
                .foregroundStyle(Color(uiColor: .fgSecondary))
                .monospacedDigit()
                .accessibilityLabel(positionLabel)

            arrow("chevron.right", target: index + 1, label: nextLabel)

            if pendingIndex != nil {
                ProgressView()
                    .controlSize(.mini)
            }

            if side == .leading { Spacer(minLength: 0) }
        }
        .padding(.top, DSSpacings.extraTiny)
    }

    private func arrow(_ symbol: String, target: Int, label: String) -> some View {
        let enabled = pendingIndex == nil && target >= 1 && target <= count

        return Button {
            onSelect(target)
        } label: {
            Image(systemName: symbol)
                .font(.system(size: 12, weight: .semibold))
                .frame(width: 28, height: 24)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .foregroundStyle(Color(uiColor: enabled ? .fgPrimary : .fgTertiary))
        .disabled(!enabled)
        .accessibilityLabel(label)
    }

    public static func == (lhs: Self, rhs: Self) -> Bool {
        lhs.index == rhs.index && lhs.count == rhs.count && lhs.pendingIndex == rhs.pendingIndex
            && lhs.side == rhs.side && lhs.positionLabel == rhs.positionLabel
    }

    public func hash(into hasher: inout Hasher) {
        hasher.combine(index)
        hasher.combine(count)
        hasher.combine(pendingIndex)
        hasher.combine(side)
        hasher.combine(positionLabel)
    }
}

/// The message of the product that a thread room answers, pinned as a quote at the top of the thread.
public struct ChatThreadRootView: View, Hashable {
    public let author: String
    public let text: String

    public init(author: String, text: String) {
        self.author = author
        self.text = text
    }

    public var body: some View {
        HStack(alignment: .top, spacing: DSSpacings.small) {
            RoundedRectangle(cornerRadius: 1.5)
                .fill(Color(uiColor: .fgPrimary))
                .frame(width: 3)

            VStack(alignment: .leading, spacing: DSSpacings.extraTiny) {
                Text(author)
                    .typography(.labelSmall)
                    .foregroundStyle(Color(uiColor: .fgSecondary))
                Text(text)
                    .typography(.bodySmall)
                    .foregroundStyle(Color(uiColor: .fgPrimary))
                    .lineLimit(4)
            }

            Spacer(minLength: 0)
        }
        .padding(DSSpacings.small)
        .background(RoundedRectangle(cornerRadius: 12).fill(Color(uiColor: .bgSurfaceContainer)))
        .padding(.vertical, DSSpacings.small)
        .accessibilityElement(children: .combine)
    }
}
