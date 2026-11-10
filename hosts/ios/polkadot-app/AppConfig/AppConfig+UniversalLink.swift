import Foundation
import Products

extension AppConfig {
    enum ProductUniversalLink {
        static let scheme = "https"

        static func url(for host: ProductHost) -> URL? {
            URL(string: "\(scheme)://\(host.toShareDomain())")
        }
    }
}
