import DesignSystem
import SwiftUI

/// The reasoning of the product, drawn as a row and not as a bubble (Chat Extension v2 additions, reasoning rows).
///
/// While the product thinks, the row shows "Thinking" with an animation and the newest six lines of the text. Then
/// the row collapses to "Thought for N s", and a tap expands it. VoiceOver reads the state of the row.
public struct ChatReasoningRowView: View, Hashable {
    public enum State: Hashable {
        case thinking
        case collapsed
        case expanded
    }

    public let state: State
    /// "Thinking", "Thought for 4 s" or "Thought".
    public let title: String
    public let text: String
    /// For example "Reasoning, collapsed".
    public let accessibilityText: String
    public let copyTitle: String
    /// It takes no part in the equality.
    public let onToggle: () -> Void
    /// It takes no part in the equality.
    public let onCopy: () -> Void

    public init(
        state: State,
        title: String,
        text: String,
        accessibilityText: String,
        copyTitle: String,
        onToggle: @escaping () -> Void,
        onCopy: @escaping () -> Void
    ) {
        self.state = state
        self.title = title
        self.text = text
        self.accessibilityText = accessibilityText
        self.copyTitle = copyTitle
        self.onToggle = onToggle
        self.onCopy = onCopy
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: DSSpacings.extraTiny) {
            header

            if state != .collapsed {
                Text(text)
                    .typography(.bodySmall)
                    .foregroundStyle(Color(uiColor: .fgSecondary))
                    .lineLimit(state == .thinking ? 6 : nil)
                    .truncationMode(.head)
                    .mask(state == .thinking ? AnyView(fade) : AnyView(Rectangle()))
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(.vertical, DSSpacings.extraSmall)
        .padding(.horizontal, DSSpacings.small)
        .overlay(alignment: .leading) {
            Rectangle()
                .fill(Color(uiColor: .fgTertiary))
                .frame(width: 2)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .contentShape(Rectangle())
        .onTapGesture {
            guard state != .thinking else { return }
            onToggle()
        }
        .contextMenu {
            Button(copyTitle, systemImage: "doc.on.doc", action: onCopy)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(accessibilityText)
        .accessibilityValue(state == .collapsed ? title : text)
        .accessibilityAddTraits(state == .thinking ? [] : .isButton)
        .accessibilityAction {
            guard state != .thinking else { return }
            onToggle()
        }
    }

    private var header: some View {
        HStack(spacing: DSSpacings.extraSmall) {
            if state == .thinking {
                Image(systemName: "ellipsis")
                    .symbolEffect(.variableColor.iterative.reversing)
            } else {
                Image(systemName: state == .expanded ? "chevron.down" : "chevron.right")
                    .font(.system(size: 10, weight: .semibold))
            }

            Text(title)
                .typography(.labelSmall)
        }
        .foregroundStyle(Color(uiColor: .fgSecondary))
    }

    /// Fades the oldest line of the streamed text.
    private var fade: some View {
        LinearGradient(
            stops: [.init(color: .clear, location: 0), .init(color: .black, location: 0.25)],
            startPoint: .top,
            endPoint: .bottom
        )
    }

    public static func == (lhs: Self, rhs: Self) -> Bool {
        lhs.state == rhs.state && lhs.title == rhs.title && lhs.text == rhs.text
            && lhs.accessibilityText == rhs.accessibilityText && lhs.copyTitle == rhs.copyTitle
    }

    public func hash(into hasher: inout Hasher) {
        hasher.combine(state)
        hasher.combine(title)
        hasher.combine(text)
        hasher.combine(accessibilityText)
    }
}
