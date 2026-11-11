import Foundation
import Products
import TrUAPIHost

protocol TrUAPIConfirmationPresenting: Sendable {
    /// Present an action review, using the bridge's requester name when the review omits it.
    func confirm(review: UserConfirmationReview, from requesterName: String) async -> Bool
    func confirmNativeCoinage(
        review: MainPurseChatPaymentReview,
        requiresPrivacyConfirmation: Bool
    ) async -> Bool
    func confirmPermission(
        review: UserConfirmationReview,
        from requesterName: String
    ) async throws -> TrUAPIPermissionDecision
}

/// Routes core-reviewed actions to the native confirmation surfaces exposed
/// through `ProductRoutersFacadeProtocol`, keyed off the typed
/// `UserConfirmationReview` so the full payload reaches each prompt.
/// Cancellation-aware: the rust core dropping its future (e.g. the product
/// closed mid-prompt) rejects immediately (throws for permission decisions);
/// an already presented prompt stays up and its late decision is discarded.
final class TrUAPIConfirmationPresenter: TrUAPIConfirmationPresenting, @unchecked Sendable {
    private let routerFacade: ProductRoutersFacadeProtocol
    private let promptMapper: TrUAPIReviewPromptMapping
    private let logger: LoggerProtocol
    private let permissionRepository: ProductPermissionRepository?

    init(
        routerFacade: ProductRoutersFacadeProtocol,
        promptMapper: TrUAPIReviewPromptMapping = TrUAPIReviewPromptMapper(),
        logger: LoggerProtocol = Logger.shared,
        permissionRepository: ProductPermissionRepository? = nil
    ) {
        self.routerFacade = routerFacade
        self.promptMapper = promptMapper
        self.logger = logger
        self.permissionRepository = permissionRepository
    }

    func confirm(review: UserConfirmationReview, from requesterName: String) async -> Bool {
        do {
            return try await dispatch(review: review, from: requesterName)
        } catch {
            logger.error("Failed to map confirmation review: \(error)")
            return false
        }
    }

    func confirmNativeCoinage(
        review: MainPurseChatPaymentReview,
        requiresPrivacyConfirmation: Bool
    ) async -> Bool {
        await confirmMainPursePayment(review, requiresPrivacyConfirmation: requiresPrivacyConfirmation)
    }

    func confirmPermission(
        review: UserConfirmationReview,
        from _: String
    ) async throws -> TrUAPIPermissionDecision {
        switch review {
        case let .identityDisclosure(identityReview):
            try await presentPermission(
                promptMapper.makePermissionRequest(from: identityReview)
            )
        case let .accountAccess(accessReview):
            try await presentPermission(
                promptMapper.makePermissionRequest(from: accessReview)
            )
        case let .accountAlias(aliasReview):
            try await presentPermission(
                promptMapper.makePermissionRequest(from: aliasReview)
            )
        case let .chatAuthority(chatReview):
            try await presentPermission(
                promptMapper.makePermissionRequest(from: chatReview)
            )
        case let .profileDisclosure(profileReview):
            try await presentPermission(
                promptMapper.makePermissionRequest(from: profileReview)
            )
        default:
            .deny
        }
    }
}

private extension TrUAPIConfirmationPresenter {
    func dispatch(review: UserConfirmationReview, from requesterName: String) async throws -> Bool {
        switch review {
        case .signPayload,
             .signRaw,
             .createTransaction:
            try await confirmSigning(for: review, from: requesterName)
        case let .mainPurseChatPayment(paymentReview):
            await confirmMainPursePayment(paymentReview)
        case let .statementStoreProductSign(statementReview):
            await confirmStatementSign(
                promptMapper.makeStatementSignRequest(from: statementReview)
            )
        case let .productSubtree(subtreeReview):
            await confirmAction(promptMapper.makeActionRequest(from: subtreeReview))
        case let .nftPurseDrain(drainReview):
            await confirmAction(promptMapper.makeActionRequest(from: drainReview))
        case .identityDisclosure,
             .chatAuthority,
             .accountAccess,
             .accountAlias,
             .profileDisclosure:
            try await confirmPermission(review: review, from: requesterName) != .deny
        case let .createProof(proofReview):
            try await confirmCreateProof(
                promptMapper.makeCreateProofRequest(from: proofReview)
            )
        case let .resourceAllocation(allocationReview):
            try await confirmAllowance(
                promptMapper.makeAllowanceRequest(from: allocationReview)
            )
        case let .signVrf(vrfReview):
            try await confirmSignVrf(
                promptMapper.makeSignVrfRequest(from: vrfReview)
            )
        }
    }

    /// Wraps the signing review as a confirm input and presents the sheet. The
    /// sheet renders the review directly (no wallet lookup); the rust core signs
    /// after approval.
    func confirmSigning(for review: UserConfirmationReview, from requesterName: String) async throws -> Bool {
        guard let input = ProductsSignConfirmInput(review: review) else {
            throw TrUAPIReviewMappingError.notASigningReview
        }

        return await presentSigning(input: input, requester: requesterName)
    }

    func presentSigning(input: ProductsSignConfirmInput, requester: String) async -> Bool {
        await awaitDecision(cancelled: false) { [routerFacade] _ in
            await withCheckedContinuation { continuation in
                let context = ProductsSignConfirmContext(
                    requester: PolkadotSigningRequester(name: requester, iconUrl: nil),
                    input: input
                )
                context.setContinuation(continuation)
                routerFacade.productsRouter.showSignConfirmation(with: context)
            }
        }
    }

    func confirmStatementSign(_ request: StatementSignConfirmationRequest) async -> Bool {
        await awaitDecision(cancelled: false) { [routerFacade] _ in
            let decision: StatementSignDecision = await withCheckedContinuation { continuation in
                let context = StatementSignConfirmationContext(request: request)
                context.setContinuation(continuation)
                routerFacade.productsRouter.showStatementSignPrompt(context: context)
            }
            return decision == .approved
        }
    }

    func confirmMainPursePayment(
        _ review: MainPurseChatPaymentReview,
        requiresPrivacyConfirmation: Bool = false
    ) async -> Bool {
        guard (try? ProductId(review.callingProductId)) != nil else { return false }
        return await awaitDecision(cancelled: false) { [routerFacade] _ in
            await withCheckedContinuation { continuation in
                let context = MainPursePaymentConfirmationContext(
                    review: review,
                    requiresPrivacyConfirmation: requiresPrivacyConfirmation
                )
                context.setContinuation(continuation)
                let prompt = MainPursePaymentPromptViewFactory.createView(context: context)
                if !routerFacade.productsRouter.present(view: prompt) {
                    context.deliver(false)
                }
            }
        }
    }

    func presentPermission(_ request: TrUAPIPermissionRequest) async throws -> TrUAPIPermissionDecision {
        try await permissionRepository?.rememberRequests(productId: request.productId, permissions: request.permissions)
        let result: Result<TrUAPIPermissionDecision, HostRejection> = await awaitDecision(
            cancelled: .failure(.Rejected(reason: "permission prompt cancelled"))
        ) { [routerFacade] _ in
            var presented = false
            let decision: Products.PermissionDecision = await withCheckedContinuation { continuation in
                let context = ProductPermissionContext(
                    productId: request.productId,
                    permissions: request.permissions
                )
                context.setContinuation(continuation)
                let prompt = ProductPermissionPromptViewFactory.createView(context: context)
                presented = routerFacade.productsRouter.present(view: prompt)
                if !presented {
                    // Resolve the UI continuation without turning failure into a user denial.
                    context.deliver(.deny)
                }
            }
            guard presented else {
                return .failure(.Rejected(reason: "permission prompt presentation unavailable"))
            }
            return .success(decision.hostDecision)
        }
        return try result.get()
    }

    func confirmAction(_ request: TrUAPIActionConfirmationRequest) async -> Bool {
        await awaitDecision(cancelled: false) { [routerFacade] _ in
            await withCheckedContinuation { continuation in
                let context = TrUAPIActionConfirmationContext(request: request)
                context.setContinuation(continuation)
                routerFacade.productsRouter.showActionConfirmation(context: context)
            }
        }
    }

    func confirmCreateProof(_ request: CreateProofConfirmationRequest) async -> Bool {
        await awaitDecision(cancelled: false) { [routerFacade] _ in
            let decision: CreateProofDecision = await withCheckedContinuation { continuation in
                let context = CreateProofConfirmationContext(request: request)
                context.setContinuation(continuation)
                routerFacade.productsRouter.showCreateProofPrompt(context: context)
            }
            return decision == .approved
        }
    }

    func confirmAllowance(_ request: TrUAPIAllowanceRequest) async throws -> Bool {
        let permissions = request.resources.compactMap { resource -> ProductPermission? in
            switch resource {
            case .statementStoreAllowance: .statementStoreAllowance(derivationIndex: nil)
            case let .productStatementStoreAllowance(dest): .statementStoreAllowance(derivationIndex: dest)
            case .bulletInAllowance, .smartContractAllowance, .autoSigning: nil
            }
        }
        try await permissionRepository?.rememberRequests(productId: request.productId, permissions: permissions)
        return await awaitDecision(cancelled: false) { [routerFacade] _ in
            let decision: AllowancePromptDecision = await withCheckedContinuation { continuation in
                let context = AllowancePromptContext(
                    productId: request.productId,
                    resources: request.resources
                )
                context.setContinuation(continuation)
                routerFacade.productsRouter.showAllowancePrompt(context: context)
            }
            return decision == .approved
        }
    }

    func confirmSignVrf(_ request: SignVrfConfirmationRequest) async -> Bool {
        await awaitDecision(cancelled: false) { [routerFacade] _ in
            let decision: SignVrfDecision = await withCheckedContinuation { continuation in
                let context = SignVrfConfirmationContext(request: request)
                context.setContinuation(continuation)
                routerFacade.productsRouter.showSignVrfPrompt(context: context)
            }
            return decision == .approved
        }
    }
}
