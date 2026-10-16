import Foundation
import Products
import KeyDerivation
import TrUAPIHost

/// Inputs for a product permission prompt raised on behalf of the rust core.
struct TrUAPIPermissionRequest: Equatable {
    let productId: ProductId
    let permissions: [ProductPermission]
}

/// Inputs for a resource allowance prompt raised on behalf of the rust core.
struct TrUAPIAllowanceRequest: Equatable {
    let productId: ProductId
    let resources: [Products.AllocatableResource]
}

/// Maps rust-core prompt reviews into the request models of the existing
/// native prompt surfaces (permission, create-proof, allowance, sign-vrf)
/// and the statement-sign prompt.
protocol TrUAPIReviewPromptMapping: Sendable {
    func makePermissionRequest(from review: IdentityDisclosureReview) -> TrUAPIPermissionRequest
    func makeActionRequest(from review: PreimageSubmitReview, requester: ProductId) -> TrUAPIActionConfirmationRequest
    func makePermissionRequest(from review: AccountAccessReview) -> TrUAPIPermissionRequest
    func makeActionRequest(from review: ProductSubtreeReview) -> TrUAPIActionConfirmationRequest
    func makePermissionRequest(from review: AccountAliasReview) -> TrUAPIPermissionRequest
    func makeCreateProofRequest(from review: CreateProofReview) throws -> CreateProofConfirmationRequest
    func makeAllowanceRequest(from review: ResourceAllocationReview) throws -> TrUAPIAllowanceRequest
    func makeSignVrfRequest(from review: SignVrfReview) throws -> SignVrfConfirmationRequest
    func makeStatementSignRequest(
        from review: StatementStoreProductSignReview
    ) -> StatementSignConfirmationRequest
}

struct TrUAPIReviewPromptMapper: TrUAPIReviewPromptMapping {
    func makePermissionRequest(from review: IdentityDisclosureReview) -> TrUAPIPermissionRequest {
        TrUAPIPermissionRequest(
            productId: review.productId,
            permissions: [.userIdentityAccess]
        )
    }

    func makeActionRequest(
        from review: PreimageSubmitReview,
        requester: ProductId
    ) -> TrUAPIActionConfirmationRequest {
        .preimageSubmit(productId: requester, size: review.size)
    }

    func makePermissionRequest(from review: AccountAccessReview) -> TrUAPIPermissionRequest {
        TrUAPIPermissionRequest(
            productId: review.requestingProductId,
            permissions: [.accountAccess(targetProductId: review.targetProductId)]
        )
    }

    func makeActionRequest(from review: ProductSubtreeReview) -> TrUAPIActionConfirmationRequest {
        .productSubtree(productId: review.productId)
    }

    /// An alias is derived within the context product's ring, so it is presented
    /// as the calling product requesting access to that product's account.
    func makePermissionRequest(from review: AccountAliasReview) -> TrUAPIPermissionRequest {
        TrUAPIPermissionRequest(
            productId: review.callingProductId,
            permissions: [.accountAccess(targetProductId: review.context.productId)]
        )
    }

    func makeCreateProofRequest(
        from review: CreateProofReview
    ) throws -> CreateProofConfirmationRequest {
        try CreateProofConfirmationRequest(
            callingProductId: review.callingProductId,
            onBehalfOfProductId: review.context.productId,
            suffix: review.context.suffix.toIndex32().bytes,
            message: review.message
        )
    }

    func makeAllowanceRequest(
        from review: ResourceAllocationReview
    ) throws -> TrUAPIAllowanceRequest {
        try TrUAPIAllowanceRequest(
            productId: review.callingProductId,
            resources: review.resources.map(makeResource)
        )
    }

    func makeSignVrfRequest(from review: SignVrfReview) throws -> SignVrfConfirmationRequest {
        try SignVrfConfirmationRequest(
            callingProductId: review.callingProductId,
            payload: SignVrfPayload(
                account: review.request.account.toAppAccount(),
                transcriptLabel: review.request.transcriptLabel,
                items: review.request.items.map(makeTranscriptItem)
            )
        )
    }

    func makeStatementSignRequest(
        from review: StatementStoreProductSignReview
    ) -> StatementSignConfirmationRequest {
        StatementSignConfirmationRequest(
            productId: review.account.dotNsIdentifier,
            payload: review.payload
        )
    }
}

private extension TrUAPIReviewPromptMapper {
    func makeResource(
        from resource: TrUAPIHostAllocatableResource
    ) throws -> Products.AllocatableResource {
        switch resource {
        case .statementStoreAllowance:
            .statementStoreAllowance
        case .bulletinAllowance:
            .bulletInAllowance
        case let .smartContractAllowance(index):
            try .smartContractAllowance(dest: index.toSelector())
        case .autoSigning:
            .autoSigning
        }
    }

    func makeTranscriptItem(
        from item: TrUAPIHostVrfTranscriptItem
    ) -> KeyDerivation.VrfTranscriptItem {
        KeyDerivation.VrfTranscriptItem(label: item.label, value: item.value)
    }
}
