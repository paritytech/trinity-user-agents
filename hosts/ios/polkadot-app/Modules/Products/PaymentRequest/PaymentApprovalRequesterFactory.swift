import Foundation
import Products

enum PaymentApprovalRequesterFactory {
    static func create(router: ProductsRouting) -> PaymentApprovalRequesting {
        PaymentApprovalRequester(router: router)
    }
}
