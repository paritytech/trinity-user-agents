import UIKit

public typealias IconLabelView = IconDetailsGenericView<UILabel>

public extension IconDetailsGenericView {
    func makeHorizontal(with mode: Mode? = nil, spacing: CGFloat? = nil) {
        stackView.axis = .horizontal

        if let spacing {
            self.spacing = spacing
        }

        if let mode {
            self.mode = mode
        }
    }

    func makeVertical(with mode: Mode? = nil, spacing: CGFloat? = nil) {
        stackView.axis = .vertical

        if let spacing {
            self.spacing = spacing
        }

        if let mode {
            self.mode = mode
        }
    }
}
