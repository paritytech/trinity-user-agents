package io.paritytech.polkadotapp.feature_w3spay_impl.deeplink

import android.net.Uri
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.domain.model.requireX25519PublicKey
import io.paritytech.polkadotapp.common.presentation.deeplink.DeepLinkHandler
import io.paritytech.polkadotapp.common.presentation.deeplink.DeeplinkProcessingOutcome
import io.paritytech.polkadotapp.common.presentation.deeplink.getQueryParameterOrThrow
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.blake2b256
import io.paritytech.polkadotapp.common.utils.decodeFormBase64UrlSafe
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.repository.awaitAccountsInitialized
import io.paritytech.polkadotapp.feature_w3spay_impl.W3sPayRouter
import io.paritytech.polkadotapp.feature_w3spay_impl.domain.W3S_MAX_DEEPLINK_AMOUNT
import io.paritytech.polkadotapp.feature_w3spay_impl.domain.W3sPaymentPayloadFactory
import io.paritytech.polkadotapp.feature_w3spay_impl.domain.parseW3sDecimalAmount
import kotlinx.coroutines.withContext
import javax.inject.Inject

/**
 * Handles `polkadotapp://pay/cheque?id=..&amount=..&key=..[&name=..]` deeplinks (from the native
 * camera or the in-app scanner). Validates the query params, derives the statement-store topic as
 * `blake2b256("pay-w3s:" || id)` and opens the existing payment screen for confirmation. The optional
 * `name` (URL-encoded, may contain spaces) is shown to the payer as the recipient; absent, the id is used.
 *
 * Lives under the `pay` host (not a `*.dot` host) so it is routed natively and never intercepted by
 * the wildcard product-SPA deeplink handler that opens any `*.dot` address in the in-app browser.
 */
internal class W3sPayDeepLinkHandler @Inject constructor(
    private val coroutineDispatchers: CoroutineDispatchers,
    private val accountRepository: AccountRepository,
    private val payloadFactory: W3sPaymentPayloadFactory,
    private val router: W3sPayRouter,
) : DeepLinkHandler {
    override suspend fun canHandle(data: Uri): Boolean {
        return data.scheme == DeepLinkHandler.APP_SCHEME && data.host == PAY_HOST && data.path == PAY_PATH
    }

    context(scope: ComputationalScope)
    override suspend fun handle(data: Uri): Result<DeeplinkProcessingOutcome> = withContext(coroutineDispatchers.io) {
        runCancellableCatching {
            accountRepository.awaitAccountsInitialized()

            val id = data.getQueryParameterOrThrow(PARAM_ID)
            require(id.isNotEmpty() && id.all(Char::isLetterOrDigit)) { "id must be a non-empty alphanumeric string" }

            val amount = requireNotNull(parseW3sDecimalAmount(data.getQueryParameterOrThrow(PARAM_AMOUNT))) {
                "amount has an invalid format"
            }
            require(amount <= W3S_MAX_DEEPLINK_AMOUNT.toBigDecimal()) { "amount must not exceed $W3S_MAX_DEEPLINK_AMOUNT" }

            // Same check W3sCoinsSubmitter applies at submit time, so validation cannot drift from
            // the key type the payload is actually encrypted to.
            val merchantKey = data.getQueryParameterOrThrow(PARAM_KEY).decodeFormBase64UrlSafe().requireX25519PublicKey()

            val topic = (TOPIC_PREFIX + id).toByteArray().blake2b256()

            // Optional human-readable recipient name (URL-encoded, may contain spaces); shown to the
            // payer instead of the raw id. Display-only: not part of the topic or the encrypted payload.
            val recipientLabel = data.getQueryParameter(PARAM_NAME)?.takeIf(String::isNotBlank) ?: id

            val payload = payloadFactory.create(amount, topic, merchantKey, paymentId = id, recipientLabel = recipientLabel)

            DeeplinkProcessingOutcome.Navigate {
                router.openW3sPayment(payload)
            }
        }
    }

    companion object {
        const val PAY_HOST = "pay"
        const val PAY_PATH = "/cheque"
        const val PARAM_ID = "id"
        const val PARAM_AMOUNT = "amount"
        const val PARAM_KEY = "key"
        const val PARAM_NAME = "name"
        private const val TOPIC_PREFIX = "pay-w3s:"
    }
}
