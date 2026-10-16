import Foundation
import Products
import TrUAPIHost

protocol TrUAPIConfirmationPresenting: Sendable {
    /// Present an action review, using the bridge's requester name when the review omits it.
    func confirm(review: UserConfirmationReview, from requesterName: String) async -> Bool
    func confirmPermission(
        review: UserConfirmationReview,
        from requesterName: String
    ) async -> TrUAPIPermissionDecision
}

/// Routes core-reviewed actions to the native confirmation surfaces exposed
/// through `ProductRoutersFacadeProtocol`, keyed off the typed
/// `UserConfirmationReview` so the full payload reaches each prompt.
/// Cancellation-aware: the rust core dropping its future (e.g. the product
/// closed mid-prompt) denies immediately; an already presented prompt
/// stays up and its late decision is discarded.
final class TrUAPIConfirmationPresenter: TrUAPIConfirmationPresenting, @unchecked Sendable {
    private let routerFacade: ProductRoutersFacadeProtocol
    private let promptMapper: TrUAPIReviewPromptMapping
    private let logger: LoggerProtocol

    init(
        routerFacade: ProductRoutersFacadeProtocol,
        promptMapper: TrUAPIReviewPromptMapping = TrUAPIReviewPromptMapper(),
        logger: LoggerProtocol = Logger.shared
    ) {
        self.routerFacade = routerFacade
        self.promptMapper = promptMapper
        self.logger = logger
    }

    func confirm(review: UserConfirmationReview, from requesterName: String) async -> Bool {
        do {
            return try await dispatch(review: review, from: requesterName)
        } catch {
            logger.error("Failed to map confirmation review: \(error)")
            return false
        }
    }

    func confirmPermission(
        review: UserConfirmationReview,
        from _: String
    ) async -> TrUAPIPermissionDecision {
        switch review {
        case let .identityDisclosure(identityReview):
            await presentPermission(
                promptMapper.makePermissionRequest(from: identityReview)
            )
        case let .accountAccess(accessReview):
            await presentPermission(
                promptMapper.makePermissionRequest(from: accessReview)
            )
        case let .accountAlias(aliasReview):
            await presentPermission(
                promptMapper.makePermissionRequest(from: aliasReview)
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
        case let .statementStoreProductSign(statementReview):
            await confirmStatementSign(
                promptMapper.makeStatementSignRequest(from: statementReview)
            )
        case let .preimageSubmit(preimageReview):
            await confirmAction(
                promptMapper.makeActionRequest(from: preimageReview, requester: requesterName)
            )
        case let .productSubtree(subtreeReview):
            await confirmAction(promptMapper.makeActionRequest(from: subtreeReview))
        case .identityDisclosure,
             .accountAccess,
             .accountAlias:
            await confirmPermission(review: review, from: requesterName) != .deny
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

    func presentSigning(input: ProductsSignConfirmInput, requester: ProductId) async -> Bool {
        await awaitDecision(cancelled: false) { [routerFacade] in
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
        await awaitDecision(cancelled: false) { [routerFacade] in
            let decision: StatementSignDecision = await withCheckedContinuation { continuation in
                let context = StatementSignConfirmationContext(request: request)
                context.setContinuation(continuation)
                routerFacade.productsRouter.showStatementSignPrompt(context: context)
            }
            return decision == .approved
        }
    }

    func presentPermission(_ request: TrUAPIPermissionRequest) async -> TrUAPIPermissionDecision {
        await awaitDecision(cancelled: .deny) { [routerFacade] in
            let decision: Products.PermissionDecision = await withCheckedContinuation { continuation in
                let context = ProductPermissionContext(
                    productId: request.productId,
                    permissions: request.permissions
                )
                context.setContinuation(continuation)
                routerFacade.productsRouter.showPrompt(context: context)
            }

            return decision.hostDecision
        }
    }

    func confirmAction(_ request: TrUAPIActionConfirmationRequest) async -> Bool {
        await awaitDecision(cancelled: false) { [routerFacade] in
            await withCheckedContinuation { continuation in
                let context = TrUAPIActionConfirmationContext(request: request)
                context.setContinuation(continuation)
                routerFacade.productsRouter.showActionConfirmation(context: context)
            }
        }
    }

    func confirmCreateProof(_ request: CreateProofConfirmationRequest) async -> Bool {
        await awaitDecision(cancelled: false) { [routerFacade] in
            let decision: CreateProofDecision = await withCheckedContinuation { continuation in
                let context = CreateProofConfirmationContext(request: request)
                context.setContinuation(continuation)
                routerFacade.productsRouter.showCreateProofPrompt(context: context)
            }
            return decision == .approved
        }
    }

    func confirmAllowance(_ request: TrUAPIAllowanceRequest) async -> Bool {
        await awaitDecision(cancelled: false) { [routerFacade] in
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
        await awaitDecision(cancelled: false) { [routerFacade] in
            let decision: SignVrfDecision = await withCheckedContinuation { continuation in
                let context = SignVrfConfirmationContext(request: request)
                context.setContinuation(continuation)
                routerFacade.productsRouter.showSignVrfPrompt(context: context)
            }
            return decision == .approved
        }
    }

    /// Bridges a prompt decision to the rust-core future, resolving exactly
    /// once. Rust-side cancellation denies without waiting for the
    /// prompt; a decision arriving afterwards is discarded.
    func awaitDecision<Decision: Sendable>(
        cancelled: Decision,
        present: @escaping @MainActor () async -> Decision
    ) async -> Decision {
        let pending = PendingDecision(cancelled: cancelled)
        return await withTaskCancellationHandler {
            await withCheckedContinuation { continuation in
                Task { @MainActor in
                    guard pending.begin(continuation) else {
                        return
                    }
                    let verdict = await present()
                    pending.finish(verdict)
                }
            }
        } onCancel: {
            Task { @MainActor in pending.finish(cancelled) }
        }
    }
}

/// Resume-once state for one confirmation. All mutable state is
/// MainActor-confined; a cancellation racing ahead of `begin` resolves the
/// incoming continuation with denial instead of leaking it.
@MainActor
private final class PendingDecision<Decision: Sendable> {
    private var isFinished = false
    private var continuation: CheckedContinuation<Decision, Never>?
    private let cancelled: Decision

    nonisolated init(cancelled: Decision) {
        self.cancelled = cancelled
    }

    /// Returns false when the confirmation already finished (e.g. cancelled
    /// before the prompt task ran); the continuation is resolved either way.
    func begin(_ continuation: CheckedContinuation<Decision, Never>) -> Bool {
        guard !isFinished else {
            continuation.resume(returning: cancelled)
            return false
        }
        self.continuation = continuation
        return true
    }

    func finish(_ verdict: Decision) {
        guard !isFinished else {
            return
        }
        isFinished = true
        continuation?.resume(returning: verdict)
        continuation = nil
    }
}
