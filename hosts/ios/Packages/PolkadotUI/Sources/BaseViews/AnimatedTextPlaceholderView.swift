import SwiftUI
import DesignSystem

public struct AnimatedTextPlaceholderView: View {
    private static let perWordDelay: TimeInterval = 0.09
    private static let wordFadeDuration: TimeInterval = 0.25
    private static let wordRiseOffset: CGFloat = 28

    private let text: String

    public init(text: String) {
        self.text = text
    }

    public var body: some View {
        let words = text.split(separator: " ").map(String.init)

        ZStack {
            Color.bgSurfaceMain.ignoresSafeArea()

            FlowLayout(spacing: DSSpacings.small) {
                ForEach(Array(words.enumerated()), id: \.offset) { index, word in
                    AnimatedWord(
                        word: word,
                        delay: Double(index) * Self.perWordDelay,
                        fadeDuration: Self.wordFadeDuration,
                        riseOffset: Self.wordRiseOffset
                    )
                }
            }
            .padding(DSSpacings.extraLarge)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

private struct AnimatedWord: View {
    let word: String
    let delay: TimeInterval
    let fadeDuration: TimeInterval
    let riseOffset: CGFloat

    @State private var opacity: Double = 0
    @State private var offset: CGFloat

    init(word: String, delay: TimeInterval, fadeDuration: TimeInterval, riseOffset: CGFloat) {
        self.word = word
        self.delay = delay
        self.fadeDuration = fadeDuration
        self.riseOffset = riseOffset
        _offset = State(initialValue: riseOffset)
    }

    var body: some View {
        Text(word)
            .typography(.displayMedium)
            .foregroundStyle(Color.fgPrimary)
            .opacity(opacity)
            .offset(y: offset)
            .onAppear {
                DispatchQueue.main.asyncAfter(deadline: .now() + delay) {
                    withAnimation(.easeInOut(duration: fadeDuration)) {
                        opacity = 1
                    }
                    withAnimation(.spring(response: 0.5, dampingFraction: 0.5)) {
                        offset = 0
                    }
                }
            }
    }
}

private struct FlowLayout: Layout {
    var spacing: CGFloat

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache _: inout ()) -> CGSize {
        let width = proposal.width ?? .infinity
        let result = arrange(width: width, subviews: subviews)
        return CGSize(width: proposal.width ?? result.size.width, height: result.size.height)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache _: inout ()) {
        let width = proposal.width ?? bounds.width
        let result = arrange(width: width, subviews: subviews)
        for (index, position) in result.positions.enumerated() {
            subviews[index].place(
                at: CGPoint(x: bounds.minX + position.x, y: bounds.minY + position.y),
                proposal: .unspecified
            )
        }
    }

    private func arrange(width: CGFloat, subviews: Subviews) -> (positions: [CGPoint], size: CGSize) {
        let sizes = subviews.map { $0.sizeThatFits(.unspecified) }

        // Group indices into rows that fit the available width.
        var rows: [[Int]] = []
        var rowWidth: CGFloat = 0
        for (index, size) in sizes.enumerated() {
            if let last = rows.last, !last.isEmpty, rowWidth + spacing + size.width <= width {
                rows[rows.count - 1].append(index)
                rowWidth += spacing + size.width
            } else {
                rows.append([index])
                rowWidth = size.width
            }
        }

        // Center each row horizontally, baseline-align by row height.
        var positions = Array(repeating: CGPoint.zero, count: sizes.count)
        var yPos: CGFloat = 0
        var maxRowWidth: CGFloat = 0
        for row in rows {
            let rowHeight = row.map { sizes[$0].height }.max() ?? 0
            let rowTotal = row.reduce(0) { $0 + sizes[$1].width } + CGFloat(row.count - 1) * spacing
            maxRowWidth = max(maxRowWidth, rowTotal)
            var xPos = max((width - rowTotal) / 2, 0)
            for index in row {
                let size = sizes[index]
                positions[index] = CGPoint(x: xPos, y: yPos + (rowHeight - size.height) / 2)
                xPos += size.width + spacing
            }
            yPos += rowHeight + spacing
        }

        return (positions, CGSize(width: maxRowWidth, height: max(yPos - spacing, 0)))
    }
}
