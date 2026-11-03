import Foundation
import Products

enum PaymentApprovalRequesterFactory {
    static func create(router: ProductsRouting, fundingProvider: FundingDomainProviding) -> PaymentApprovalRequesting {
        AutoAllowPaymentApprovalRequester(
            allowedLabels: ProductAutoAllowList.labels(fundingProvider: fundingProvider),
            wrapped: PaymentApprovalRequester(router: router)
        )
    }
}
