import Foundation
import PolkadotUI
@testable import polkadot_app

/// Records the product domains that a view asks icons for, and loads no image.
final class RecordingProductIconViewModelFactory: ProductIconViewModelMaking {
    private(set) var requestedDomains: [String] = []

    func createViewModel(for domain: String) -> any ImageViewModelProtocol {
        requestedDomains.append(domain)
        return StaticImageViewModel(image: nil)
    }
}
