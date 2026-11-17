import BackgroundExecution
import Foundation
import Coinage
import CryptoKit
import KeyDerivation
import MessageExchangeKit
import NovaCrypto
import Operation_iOS
import SDKLogger
import StatementStore
import SubstrateOperation
import SubstrateSdk

/// Identity and display attributes of a single W3S payment.
struct W3sPaymentDetails {
    let paymentId: String
    let topic: Data
    let merchantKey: Data
    let merchantName: String?
    let amountString: String
    let chainAssetId: String
}

final class W3sStatementSubmitter {
    private let details: W3sPaymentDetails
    private let wallet: WalletManaging
    private let statementStoreSubmitter: StatementStoreSubmitting
    private let historyStore: W3sPaymentHistoryStoring
    private let blockInfoProvider: BlockInfoProviding
    private let priorityFactory: StatementPriorityMaking
    private let backgroundExecutor: any BackgroundExecuting
    private let logger: SDKLoggerProtocol?

    init(
        details: W3sPaymentDetails,
        wallet: WalletManaging,
        statementStoreSubmitter: StatementStoreSubmitting,
        historyStore: W3sPaymentHistoryStoring,
        blockInfoProvider: BlockInfoProviding,
        priorityFactory: StatementPriorityMaking = StatementPriorityFactory(),
        backgroundExecutor: any BackgroundExecuting,
        logger: SDKLoggerProtocol? = nil
    ) {
        self.details = details
        self.wallet = wallet
        self.statementStoreSubmitter = statementStoreSubmitter
        self.historyStore = historyStore
        self.blockInfoProvider = blockInfoProvider
        self.priorityFactory = priorityFactory
        self.backgroundExecutor = backgroundExecutor
        self.logger = logger
    }
}

extension W3sStatementSubmitter: TransferSubmitting {
    func exportId(messageId _: Chat.MessageId) throws -> CoinageOperationId {
        try W3sPaymentRecord.exportId(paymentId: details.paymentId)
    }

    func sendTransfer(
        _ memo: TransferMemo,
        to _: AccountId,
        messageId _: Chat.MessageId,
        commit: @escaping @Sendable () async throws -> Void
    ) async throws {
        // Save pending record immediately for crash-resilience and recovery UX.
        // Memo entries retained so payment can be revoked later. History is
        // auxiliary — a persistence failure must not abort the payment.
        let submittedAtBlock = try? await blockInfoProvider.fetchFinalized()
        await persist {
            try await self.historyStore.save(
                self.makePendingRecord(memo: memo, submittedAtBlock: submittedAtBlock)
            )
        }

        // Committed before the statement carries the keys away; inside the same assertion as the submit,
        // so the window between the two is never suspended.
        try await backgroundExecutor.execute {
            try await commit()
            try await self.submitStatement(memo: memo)
        }
    }
}

private extension W3sStatementSubmitter {
    func submitStatement(memo: TransferMemo) async throws {
        do {
            let envelope = try buildEnvelope(memo: memo)
            let envelopeBytes = try envelope.scaleEncoded()
            let signer = try makeSigner()

            let scaleEncodedPayload = try envelopeBytes.scaleEncoded()

            // Encoded priority carries the protocol-epoch second in the low 32 bits,
            // so adding 120 advances expiry by 2 minutes.
            let builder = StatementSubmitParametersBuilder(signer: signer, logger: logger)
                .addTopic1(details.topic)
                .addExpiry(priorityFactory.makeTimestampPriority() + 120)
                .addScaleEncodedPayload(scaleEncodedPayload)

            try await statementStoreSubmitter.submitStatement(with: builder)

            await persist {
                try await self.historyStore.updateStatus(
                    paymentId: self.details.paymentId,
                    status: .submitted
                )
            }
        } catch {
            // A submit error here is often a false-negative (the statement landed, the response was
            // lost), so never mark the payment failed — leave the record for the tracking service to
            // reconcile against chain truth. Still rethrow: this submitter is fatal for the flow, and
            // the caller takes the coins back rather than leaving them with a statement that may never
            // have left.
            logger?.error("W3S payment \(details.paymentId) statement submission failed: \(error)")
            throw error
        }
    }

    func makePendingRecord(memo: TransferMemo, submittedAtBlock: BlockNumber?) -> W3sPaymentRecord {
        let now = Date()
        return W3sPaymentRecord(
            paymentId: details.paymentId,
            recipientTopic: details.topic,
            merchantName: details.merchantName,
            merchantPublicKey: details.merchantKey,
            amountString: details.amountString,
            chainAssetId: details.chainAssetId,
            memo: memo,
            submittedAtBlock: submittedAtBlock,
            createdAt: now,
            updatedAt: now,
            status: .pending
        )
    }

    func persist(_ write: () async throws -> Void) async {
        do {
            try await write()
        } catch {
            logger?.error("W3S payment \(details.paymentId) history write failed: \(error)")
        }
    }

    func buildEnvelope(memo: TransferMemo) throws -> W3sPaymentEnvelope {
        let payload = W3sPaymentPayload(
            amount: details.amountString,
            timestampMs: UInt64(Date().timeIntervalSince1970 * 1_000),
            coins: memo.entries,
            paymentId: details.paymentId
        )
        let plaintext = try payload.scaleEncoded()

        let ephemeralPrivateKey = Curve25519.KeyAgreement.PrivateKey()
        let encryptorFactory = X25519ChaChaPolyEncryptorFactory(privateKey: ephemeralPrivateKey)
        let encryptor = try encryptorFactory.makeEncryptor(
            remotePublicKey: details.merchantKey
        )
        let ciphertext = try encryptor.encrypt(plaintext)

        return W3sPaymentEnvelope(
            encryptedData: ciphertext,
            ephemeralPublicKey: encryptorFactory.localPublicKey
        )
    }

    func makeSigner() throws -> StatementStoreKeypairSigner {
        let rawPublicKey = try wallet.getRawPublicKey()
        let publicKey = try SNPublicKey(rawData: rawPublicKey)
        let rawSecretKey = try wallet.fetchSignerSecret(for: publicKey)
        let secretKey = try SNPrivateKey(rawData: rawSecretKey)
        let keypair = SNKeypair(privateKey: secretKey, publicKey: publicKey)
        return StatementStoreKeypairSigner(keypair: keypair)
    }
}
