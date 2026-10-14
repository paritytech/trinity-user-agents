package io.paritytech.polkadotapp.feature_w3spay_impl.presentation.scan

import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.feature_scan_api.domain.PostParseAction
import io.paritytech.polkadotapp.feature_scan_api.domain.ScanContentParser
import io.paritytech.polkadotapp.feature_w3spay_impl.W3sPayRouter
import io.paritytech.polkadotapp.feature_w3spay_impl.data.config.W3sMerchantConfigRepository
import io.paritytech.polkadotapp.feature_w3spay_impl.domain.W3sPaymentPayloadFactory
import io.paritytech.polkadotapp.feature_w3spay_impl.domain.W3sScanError
import io.paritytech.polkadotapp.feature_w3spay_impl.domain.dsfinvk.DsFinVkReceiptParser
import timber.log.Timber
import javax.inject.Inject

/**
 * Lets the main wallet QR scanner accept DSFinV-K cash-register receipt QR codes. Only claims codes
 * that parse as a well-formed Kassenbeleg-V1 receipt; resolves the merchant ("cash register") from
 * the `w3s-merchants` remote config and opens the existing payment screen for confirmation.
 *
 * Every failure is logged with its distinct [W3sScanError] reason, because the scanner surfaces only
 * a generic "invalid code" message to the user.
 */
internal class DsFinVkScanContentParser @Inject constructor(
    private val merchantConfigRepository: W3sMerchantConfigRepository,
    private val payloadFactory: W3sPaymentPayloadFactory,
    private val router: W3sPayRouter,
) : ScanContentParser {
    override suspend fun canHandle(content: String): Boolean {
        return DsFinVkReceiptParser.parse(content).isSuccess
    }

    context(scope: ComputationalScope)
    override suspend fun handle(content: String): Result<PostParseAction> {
        val receipt = DsFinVkReceiptParser.parse(content)
            .getOrElse { cause -> return rejected(W3sScanError.UnreadableReceipt(cause)) }

        val merchant = merchantConfigRepository.merchantFor(receipt.serialNumber)
            .getOrElse { cause -> return rejected(W3sScanError.MerchantConfigUnavailable(receipt.serialNumber, cause)) }
            ?: return rejected(W3sScanError.UnknownMerchant(receipt.serialNumber))

        val paymentId = "${receipt.serialNumber}/${receipt.transactionNumber}"
        val recipientLabel = merchant.name ?: receipt.serialNumber
        return runCatching {
            val payload = payloadFactory.create(receipt.amount, merchant.topic, merchant.key, paymentId, recipientLabel)
            PostParseAction.BackAndThen { router.openW3sPayment(payload) }
        }.onFailure { Timber.e(it, "W3S scan rejected: failed to build payment payload (paymentId=$paymentId)") }
    }

    private fun rejected(error: W3sScanError): Result<PostParseAction> {
        Timber.w(error, "W3S scan rejected: ${error.message}")
        return Result.failure(error)
    }
}
