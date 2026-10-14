import Foundation
import Products
@testable import polkadot_app

final class MockNativeApiFactory: ProductsNativeApiMaking {
    func makeApi(
        messagingSupport _: ProductsNativeApi.MessagingSupport?,
        productId _: ProductId,
        routers _: ProductRoutersFacadeProtocol
    ) -> any ProductsNativeApiProtocol {
        StubProductsNativeApi()
    }
}
