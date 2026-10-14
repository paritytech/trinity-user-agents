import UIKit

enum BlurHashConfiguration {
    static let components = (horizontal: 4, vertical: 3)
    static let encodingMaximumSide: CGFloat = 128
    static let encodingPreviewSize = CGSize(width: encodingMaximumSide, height: encodingMaximumSide)
    static let decodingPreviewMaximumSide: CGFloat = 32
}
