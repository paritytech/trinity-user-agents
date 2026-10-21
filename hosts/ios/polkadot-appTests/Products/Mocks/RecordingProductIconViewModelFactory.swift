import Foundation
import PolkadotUI
@testable import polkadot_app

/// Records the product domains and the chat resources that a view asks icons for, and loads no image.
final class RecordingProductIconViewModelFactory: ProductIconViewModelMaking {
    private(set) var requestedDomains: [String] = []
    private(set) var requestedResources: [ChatResource] = []

    func createViewModel(for domain: String) -> any ImageViewModelProtocol {
        requestedDomains.append(domain)
        return StaticImageViewModel(image: nil)
    }

    func createViewModel(forResource _: String) -> (any ImageViewModelProtocol)? {
        nil
    }

    func createViewModel(forChatResource resource: ChatResource) -> any ImageViewModelProtocol {
        requestedResources.append(resource)
        return StaticImageViewModel(image: nil)
    }
}
