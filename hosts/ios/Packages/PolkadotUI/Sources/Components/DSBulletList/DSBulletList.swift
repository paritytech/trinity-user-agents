import DesignSystem
import SwiftUI

/// A vertical list of bullet points: a dot hung in a fixed-width gutter so every line of an item, wrapped
/// or not, starts at the same indent. The dot is text, so it sits on the first line's baseline and scales
/// with Dynamic Type alongside the item.
///
/// VoiceOver reads the list as one element, one item per line, so the reader does not have to swipe
/// through each bullet separately.
public struct DSBulletList: View {
    private let items: [String]
    private let typography: TypographyStyle
    private let color: Color
    private let indent: CGFloat
    private let spacing: CGFloat

    /// - Parameters:
    ///   - indent: distance from the list's leading edge to the start of each item's text.
    ///   - spacing: vertical gap between items.
    public init(
        _ items: [String],
        typography: TypographyStyle = .paragraphMedium,
        color: Color = .fgSecondary,
        indent: CGFloat = 20,
        spacing: CGFloat = DSSpacings.tiny
    ) {
        self.items = items
        self.typography = typography
        self.color = color
        self.indent = indent
        self.spacing = spacing
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: spacing) {
            ForEach(Array(items.enumerated()), id: \.offset) { _, item in
                HStack(alignment: .firstTextBaseline, spacing: 0) {
                    Text(verbatim: "\u{2022}")
                        .typography(typography)
                        .frame(width: indent - DSSpacings.small, alignment: .trailing)
                        .padding(.trailing, DSSpacings.small)

                    Text(item)
                        .typography(typography)
                        .fixedSize(horizontal: false, vertical: true)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
        }
        .foregroundStyle(color)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(items.joined(separator: "\n"))
    }
}

#Preview {
    DSBulletList([
        "Minimises waiting time",
        "Funds typically ready in seconds",
        "Most funds ready in seconds, the rest in minutes, and this one wraps onto a second line"
    ])
    .padding()
    .background(Color.bgSurfaceNested)
}
