import UIKit

extension StackTableView {
    convenience init() {
        self.init(frame: .zero, style: StackTableViewStyle.defaultStyle)

        contentInsets = UIEdgeInsets(top: 24, left: 16, bottom: 24, right: 16)
    }
}

public extension StackTableViewStyle {
    static var defaultStyle: StackTableViewStyle {
        .init(fillColor: UIColor(resource: .backgroundSecondary), cornerRadius: 32)
    }

    static var fill6Style: StackTableViewStyle {
        .init(fillColor: UIColor(resource: .fill6), cornerRadius: 32)
    }

    static var clearStyle: StackTableViewStyle {
        .init(fillColor: .clear, cornerRadius: 0)
    }
}
