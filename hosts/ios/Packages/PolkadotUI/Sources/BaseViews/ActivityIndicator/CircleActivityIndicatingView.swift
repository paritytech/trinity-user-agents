import UIKit

extension ActivityIndicatorView: LoadIndicatorRepresentable {
    public func startAnimating() {
        startAnimating(after: .zero)
    }

    public var isAnimating: Bool {
        loadingView.isAnimating
    }
}

public class CircleActivityIndicatingView<Content: UIView>: GenericLoadableView<Content, ActivityIndicatorView> {}
