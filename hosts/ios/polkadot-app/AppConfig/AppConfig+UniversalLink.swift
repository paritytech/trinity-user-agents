import Foundation

extension AppConfig {
    enum ProductUniversalLink {
        static let scheme = "https"
        static var shareRoot: String { Brand.shareRoot }

        static func url(for name: String) -> URL? {
            URL(string: "\(scheme)://\(name).\(shareRoot)")
        }
    }
}
