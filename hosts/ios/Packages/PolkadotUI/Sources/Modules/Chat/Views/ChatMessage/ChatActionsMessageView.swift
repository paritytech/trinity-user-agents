import DesignSystem
import SwiftUI

/// A product message with buttons that the host draws. A press reports the action identifier.
public struct ChatActionsMessageView: View, Hashable {
    public static let reuseIdentifier = "ChatActionsMessageView"

    public enum Layout: Hashable {
        case column
        case grid
    }

    public struct Button: Hashable, Identifiable {
        public let id: String
        public let title: String

        public init(id: String, title: String) {
            self.id = id
            self.title = title
        }
    }

    private enum Constants {
        static let gridColumns = 2
    }

    let text: AttributedString?
    let buttons: [Button]
    let layout: Layout
    let onPress: (String) -> Void

    public init(
        text: AttributedString?,
        buttons: [Button],
        layout: Layout,
        onPress: @escaping (String) -> Void
    ) {
        self.text = text
        self.buttons = buttons
        self.layout = layout
        self.onPress = onPress
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: DSSpacings.small) {
            if let text {
                Text(text)
                    .typography(.paragraphLarge)
                    .foregroundStyle(.fgPrimary)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }

            switch layout {
            case .column:
                VStack(spacing: DSSpacings.extraSmall) {
                    buttonViews
                }
            case .grid:
                LazyVGrid(
                    columns: Array(
                        repeating: GridItem(.flexible(), spacing: DSSpacings.extraSmall),
                        count: Constants.gridColumns
                    ),
                    spacing: DSSpacings.extraSmall
                ) {
                    buttonViews
                }
            }
        }
        .padding(DSSpacings.small + DSSpacings.tiny)
    }

    public static func == (lhs: Self, rhs: Self) -> Bool {
        lhs.text == rhs.text && lhs.buttons == rhs.buttons && lhs.layout == rhs.layout
    }

    public func hash(into hasher: inout Hasher) {
        hasher.combine(text)
        hasher.combine(buttons)
        hasher.combine(layout)
    }
}

private extension ChatActionsMessageView {
    var buttonViews: some View {
        ForEach(buttons) { button in
            SwiftUI.Button {
                onPress(button.id)
            } label: {
                Text(button.title)
                    .lineLimit(2)
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.ds(style: .secondary, shape: .rounded, size: .medium))
        }
    }
}

#if DEBUG
    #Preview(traits: .sizeThatFitsLayout) {
        VStack(spacing: 20) {
            ChatActionsMessageView(
                text: AttributedString("Pick a side"),
                buttons: [.init(id: "a", title: "Red"), .init(id: "b", title: "Blue")],
                layout: .column,
                onPress: { _ in }
            )
            ChatActionsMessageView(
                text: nil,
                buttons: (1 ... 4).map { .init(id: "\($0)", title: "Option \($0)") },
                layout: .grid,
                onPress: { _ in }
            )
        }
        .background(Color(.bgSurfaceContainer))
    }
#endif
