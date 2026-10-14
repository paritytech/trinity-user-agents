import UIKit

extension NSDirectionalEdgeInsets: @retroactive Hashable {
    public func hash(into hasher: inout Hasher) {
        hasher.combine(leading)
        hasher.combine(trailing)
        hasher.combine(top)
        hasher.combine(bottom)
    }
}

extension NSDirectionalEdgeInsets {
    static func all(insets: CGFloat) -> Self {
        NSDirectionalEdgeInsets(top: insets, leading: insets, bottom: insets, trailing: insets)
    }

    init(horizontal: CGFloat = .zero, vertical: CGFloat = .zero) {
        self.init()
        leading = horizontal
        trailing = horizontal
        top = vertical
        bottom = vertical
    }
}
