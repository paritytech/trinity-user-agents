import Foundation
import Products

protocol SignVrfConfirming: Sendable {
    func confirm(_ request: SignVrfConfirmationRequest) async -> SignVrfDecision
}

actor SignVrfConfirmationRequester: SignVrfConfirming {
    private let router: ProductsRouting

    init(router: ProductsRouting) {
        self.router = router
    }

    func confirm(_ request: SignVrfConfirmationRequest) async -> SignVrfDecision {
        await withCheckedContinuation { continuation in
            Task { @MainActor [router] in
                let context = SignVrfConfirmationContext(request: request)
                context.setContinuation(continuation)
                router.showSignVrfPrompt(context: context)
            }
        }
    }
}
