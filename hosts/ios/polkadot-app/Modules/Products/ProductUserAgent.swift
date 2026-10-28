import Foundation
import UIKit

enum ProductUserAgent {
    static var applicationName: String {
        applicationName(
            version: Bundle.main.appVersion,
            build: Bundle.main.appBuild,
            model: UIDevice.current.modelIdentifier
        )
    }

    static func applicationName(version: String?, build: String?, model: String) -> String {
        let product = version.map { "PolkadotApp/\($0)" } ?? "PolkadotApp"
        let details = [build, model].compactMap { $0 }.joined(separator: "; ")
        return details.isEmpty ? product : "\(product) (\(details))"
    }
}
