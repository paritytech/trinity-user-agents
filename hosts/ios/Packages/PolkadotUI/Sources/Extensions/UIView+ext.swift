import UIKit

public extension UIView {
    static func create<View: UIView>(with mutation: (View) -> Void) -> View {
        let view = View()
        mutation(view)
        return view
    }
}

public extension (any UIContentView & UIView)? {
    @MainActor mutating func apply(
        _ configuration: UIContentConfiguration?,
        addViewClosure: (UIView) -> Void
    ) {
        guard let configuration else {
            self?.removeFromSuperview()
            self = .none
            return
        }
        if case let .some(contentView) = self, !contentView.supports(configuration) {
            contentView.removeFromSuperview()
            self = .none
        }

        guard case let .some(contentView) = self else {
            let contentView = configuration.makeContentView()
            self = .some(contentView)

            addViewClosure(contentView)

            return
        }

        contentView.configuration = configuration
    }
}

public extension UIView {
    func setHidden(_ isHidden: Bool) {
        if self.isHidden != isHidden {
            self.isHidden = isHidden
        }
    }
}
