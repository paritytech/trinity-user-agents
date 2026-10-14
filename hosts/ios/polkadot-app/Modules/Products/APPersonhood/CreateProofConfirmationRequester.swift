import Foundation
import Products

protocol CreateProofConfirming: Sendable {
    func confirm(_ request: CreateProofConfirmationRequest) async -> CreateProofDecision
}

/// Serializes create-proof confirmation prompts so that only one sheet
/// is presented at a time.
actor CreateProofConfirmationRequester: CreateProofConfirming {
    private let router: ProductsRouting

    init(router: ProductsRouting) {
        self.router = router
    }

    func confirm(_ request: CreateProofConfirmationRequest) async -> CreateProofDecision {
        await withCheckedContinuation { continuation in
            Task { @MainActor [router] in
                let context = CreateProofConfirmationContext(request: request)
                context.setContinuation(continuation)
                router.showCreateProofPrompt(context: context)
            }
        }
    }
}
