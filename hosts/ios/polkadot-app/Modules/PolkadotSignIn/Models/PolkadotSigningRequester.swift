import Foundation
import Products

struct PolkadotSigningRequester {
    let name: String
    /// The product whose icon the signing sheet shows.
    let productId: ProductId?
    /// Name of the paired device that relayed the request, when it arrived over SSO.
    let pairedDeviceName: String?
}

extension PolkadotSigningRequester {
    init(productId: ProductId, pairedDeviceName: String?) {
        self.init(name: productId, productId: productId, pairedDeviceName: pairedDeviceName)
    }
}
