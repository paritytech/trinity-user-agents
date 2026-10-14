import UIKit

extension UIColor {
    var isLight: Bool {
        var red: CGFloat = 0
        var green: CGFloat = 0
        var blue: CGFloat = 0
        var alpha: CGFloat = 0
        getRed(&red, green: &green, blue: &blue, alpha: &alpha)
        return (0.299 * red + 0.587 * green + 0.114 * blue) > 0.5
    }
}
