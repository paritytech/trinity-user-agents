package io.paritytech.polkadotapp.feature_w3spay_impl.domain

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.decodeFromByteArray
import io.paritytech.polkadotapp.common.domain.model.requireX25519PublicKey
import io.paritytech.polkadotapp.common.domain.model.scale.toScale
import io.paritytech.polkadotapp.common.utils.encodeToByteArrayCatching
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.TransferMemo
import io.paritytech.polkadotapp.feature_coinage_api.domain.submitter.CoinsSubmitter
import io.paritytech.polkadotapp.feature_statement_store_api.data.Statement
import io.paritytech.polkadotapp.feature_statement_store_api.data.StatementStoreService
import io.paritytech.polkadotapp.feature_statement_store_api.data.encryption.CommunicationEncryption
import io.paritytech.polkadotapp.feature_statement_store_api.domain.StatementStoreMessageProver
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.StatementExpiry
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.StatementTimestamp
import io.paritytech.polkadotapp.feature_statement_store_api.domain.prepareSignedStatement
import io.paritytech.polkadotapp.feature_w3spay_impl.data.scale.W3sEncryptedPayloadV1
import io.paritytech.polkadotapp.feature_w3spay_impl.data.scale.W3sPaymentDataV1
import io.paritytech.polkadotapp.feature_w3spay_impl.data.scale.W3sSubmitterPayload
import java.math.BigDecimal
import java.math.RoundingMode
import javax.inject.Inject

/** Submitter id under which [W3sCoinsSubmitter] is registered. */
internal const val W3S_COINS_SUBMITTER_ID = "w3s-pay"

/**
 * Delivers a coinage transfer memo to a W3S merchant: decodes the merchant topic/key/paymentId from
 * the opaque submitter payload, encrypts the payment to the merchant's X25519 key using the SSO ECIES
 * scheme ([CommunicationEncryption.Factory.createOneTimeUse]) and publishes it as a wallet-signed
 * statement under the merchant topic.
 */
class W3sCoinsSubmitter @Inject constructor(
    private val accountRepository: AccountRepository,
    private val proverFactory: StatementStoreMessageProver.Factory,
    private val statementStoreService: StatementStoreService,
    private val encryptionFactory: CommunicationEncryption.Factory,
) : CoinsSubmitter {
    private companion object {
        const val EXPIRY_PRIORITY_OFFSET_SECONDS = 120L
    }

    override suspend fun submit(memo: TransferMemo, amount: BigDecimal, submitterPayload: ByteArray): Result<Unit> {
        return runCatching { BinaryScale.decodeFromByteArray<W3sSubmitterPayload>(submitterPayload) }
            .flatMap { payload -> encrypt(payload, memo, amount).map { payload.topic to it } }
            .flatMap { (topic, data) -> buildSignedStatement(topic, data) }
            .flatMap { statement -> statementStoreService.submitStatement(statement) }
    }

    private suspend fun encrypt(
        payload: W3sSubmitterPayload,
        memo: TransferMemo,
        amount: BigDecimal,
    ): Result<ByteArray> {
        val paymentData = W3sPaymentDataV1(
            amount = amount.setScale(2, RoundingMode.HALF_UP).toPlainString(),
            timestamp = System.currentTimeMillis().toULong(),
            coins = memo.coins.map { it.privateKey.value },
            id = payload.paymentId,
        )

        return BinaryScale.encodeToByteArrayCatching(paymentData)
            .mapCatching { plaintext ->
                val encryption = encryptionFactory.createOneTimeUse(payload.merchantKey.requireX25519PublicKey())
                W3sEncryptedPayloadV1(
                    encryptedData = encryption.encrypt(plaintext),
                    ephemeralPublicKey = encryption.localPublicKey.toScale(),
                )
            }
            .flatMap { BinaryScale.encodeToByteArrayCatching(it) }
    }

    private suspend fun buildSignedStatement(topic: ByteArray, data: ByteArray): Result<Statement> {
        val walletAccount = accountRepository.getWalletAccount()
        val prover = proverFactory.createKeyPairProver(walletAccount)

        val priority = (StatementTimestamp.currentEpochSeconds() + EXPIRY_PRIORITY_OFFSET_SECONDS).toUInt()
        val body = Statement.Body(
            channel = null,
            expiry = StatementExpiry.createWithPriority(priority),
            topic1 = topic,
            data = data,
        )

        return runCatching { prover.prepareSignedStatement(body) }
    }
}
