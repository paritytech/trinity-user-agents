import UIKit

public extension UIImage {
    /// The Coinage mark. Exposed because the asset symbols generated for this
    /// package are internal, and the amount input lives in the app target.
    static var cashLogo: UIImage { UIImage(resource: .iconCashLogo) }

    /// The Collectibles book cover at its natural size, which the Pocket peek and the book screen both draw.
    static var collectiblesBook: UIImage { UIImage(resource: .imageCollectibles) }
}
