import Foundation
import KeyDerivation
import Products
import StructuredConcurrency
import SubstrateSdk

// MARK: - Signing

extension ProductsNativeApi {
    func signVrf(_ payload: SignVrfPayload) async throws -> VrfSignature {
        try await signVrfHandler.signVrf(payload)
    }

    func signPayload(_ payload: SignTransactionPayload<ProductAccountId>) async throws -> SignResult {
        let signingRequest = PolkadotHostRemoteMessage.SigningRequest.transaction(payload)
        let model: PolkadotHostSigningModel = .signingRequest(signingRequest)

        let context = ProductSigningContext(
            requesterName: productId,
            signingModel: model
        )

        logger.debug("Proceeding with signing payload")

        return try await withCheckedThrowingContinuation { continuation in
            context.setSignResultContinuation(continuation)
            Task {
                do {
                    try await self.signingHandler.sponsorAndPresent(
                        model: model,
                        context: context
                    )
                } catch {
                    logger.error("Sign payload failed: \(error)")
                    try? await context.rejectRequest()
                }
            }
        }
    }

    func signRaw(_ payload: SigningRawPayload) async throws -> SignResult {
        let rawPayload = payload.toHostSigningRawPayload()

        let signingRequest = PolkadotHostRemoteMessage.SigningRequest.rawPayload(rawPayload)
        let model: PolkadotHostSigningModel = .signingRequest(signingRequest)

        let context = ProductSigningContext(
            requesterName: productId,
            signingModel: model
        )

        logger.debug("Proceeding with signing raw data")

        return try await withCheckedThrowingContinuation { continuation in
            context.setSignResultContinuation(continuation)
            Task {
                do {
                    try await self.signingHandler.sponsorAndPresent(
                        model: model,
                        context: context
                    )
                } catch {
                    logger.error("Sign raw failed: \(error)")
                    try? await context.rejectRequest()
                }
            }
        }
    }

    func createTransaction(
        _ payload: CreateTransactionPayload<ProductAccountId>
    ) async throws -> CreateTransactionResult {
        let model: PolkadotHostSigningModel = .createTransaction(payload)

        let context = ProductSigningContext(
            requesterName: productId,
            signingModel: model
        )

        logger.debug("Proceeding with creating transaction")

        return try await withCheckedThrowingContinuation { continuation in
            context.setCreateTransactionContinuation(continuation)
            Task {
                do {
                    try await self.signingHandler.sponsorAndPresent(
                        model: model,
                        context: context
                    )
                } catch {
                    logger.error("Create transaction failed: \(error)")
                    try? await context.rejectRequest()
                }
            }
        }
    }

    func signPayloadLegacy(_ payload: SignTransactionPayload<SS58Account>) async throws -> SignResult {
        let model: PolkadotHostSigningModel = .legacySignPayload(payload: payload)

        let context = ProductSigningContext(
            requesterName: productId,
            signingModel: model
        )

        logger.debug("Proceeding with legacy signing payload")

        return try await withCheckedThrowingContinuation { continuation in
            context.setSignResultContinuation(continuation)
            Task {
                do {
                    try self.accountResolver.resolveWallet(for: payload.account.accountId)
                    try await self.signingHandler.sponsorAndPresent(
                        model: model,
                        context: context
                    )
                } catch {
                    logger.error("Legacy sign payload failed: \(error)")
                    try? await context.rejectRequest()
                }
            }
        }
    }

    func signRawLegacy(_ payload: SignRawLegacyPayload) async throws -> SignResult {
        let type: PolkadotHostRemoteMessage.SigningRawPayload.PayloadType =
            switch payload.content {
            case let .bytes(data): .bytes(data)
            case let .payload(string): .payload(string)
            }

        let model: PolkadotHostSigningModel = .legacyRawPayload(account: payload.account, type: type)

        let context = ProductSigningContext(
            requesterName: productId,
            signingModel: model
        )

        logger.debug("Proceeding with legacy signing raw data")

        return try await withCheckedThrowingContinuation { continuation in
            context.setSignResultContinuation(continuation)
            Task {
                do {
                    try self.accountResolver.resolveWallet(for: payload.account)
                    try await self.signingHandler.sponsorAndPresent(
                        model: model,
                        context: context
                    )
                } catch {
                    logger.error("Legacy sign raw failed: \(error)")
                    try? await context.rejectRequest()
                }
            }
        }
    }

    func createTransactionLegacy(
        _ payload: CreateTransactionPayload<LegacyAccountId>
    ) async throws -> CreateTransactionResult {
        let model: PolkadotHostSigningModel = .legacyCreateTransaction(payload: payload)

        let context = ProductSigningContext(
            requesterName: productId,
            signingModel: model
        )

        logger.debug("Proceeding with legacy create transaction")

        return try await withCheckedThrowingContinuation { continuation in
            context.setCreateTransactionContinuation(continuation)
            Task {
                do {
                    try self.accountResolver.resolveWallet(for: payload.signer.accountId)
                    try await self.signingHandler.sponsorAndPresent(
                        model: model,
                        context: context
                    )
                } catch {
                    logger.error("Legacy create transaction failed: \(error)")
                    try? await context.rejectRequest()
                }
            }
        }
    }
}
