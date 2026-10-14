import Foundation

private class BundleProvider {
    static let bundle = Bundle(for: BundleProvider.self)
}

extension Bundle {
    static var current: Bundle { BundleProvider.bundle }
}
