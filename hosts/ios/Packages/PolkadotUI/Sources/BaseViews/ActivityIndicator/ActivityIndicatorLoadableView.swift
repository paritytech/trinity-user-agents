import UIKit

extension UIActivityIndicatorView: LoadIndicatorRepresentable {}

public class ActivityIndicatorLoadableView<Content: UIView>: GenericLoadableView<Content, UIActivityIndicatorView> {}
