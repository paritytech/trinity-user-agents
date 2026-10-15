import DesignSystem
import SwiftUI

/// A long text of a product shows only its start, with a "Show more" button (02-DESIGN §6.1). The host keeps
/// which texts the user expanded, so a text stays expanded when its cell draws again.
public struct ChatTextCollapse {
    /// The number of characters that a collapsed text shows. Android uses the same bound.
    public static let collapsedLength = 3_000

    let title: String
    let onExpand: () -> Void

    /// - Parameters:
    ///   - title: The title of the button, for example "Show more".
    ///   - onExpand: Expands the text. The host then draws the message again without a collapse.
    public init(title: String, onExpand: @escaping () -> Void) {
        self.title = title
        self.onExpand = onExpand
    }

    /// The start of the text and an ellipsis, or nil when the text is not longer than `length`.
    public static func collapsed(_ text: AttributedString, length: Int = collapsedLength) -> AttributedString? {
        guard text.characters.count > length else {
            return nil
        }

        let end = text.characters.index(text.startIndex, offsetBy: length)
        var start = AttributedString(text[text.startIndex ..< end])
        start.append(AttributedString("…"))
        return start
    }
}

/// The "Show more" button under a collapsed text.
struct ChatShowMoreButton: View {
    let title: String
    let color: Color
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Text(title)
                .scaledTypography(.bodyMediumEmphasized)
                .foregroundStyle(color)
                .frame(maxWidth: .infinity, alignment: .leading)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}
