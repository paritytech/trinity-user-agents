import UIKit

public extension UIImage {
    /// The Coinage mark. Exposed because the asset symbols generated for this
    /// package are internal, and the amount input lives in the app target.
    static var cashLogo: UIImage { UIImage(resource: .iconCashLogo) }
}
