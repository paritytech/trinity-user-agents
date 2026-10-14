package io.paritytech.polkadotapp.feature_products_impl.domain.signTransaction

import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.feature_products_api.model.signing.SignedTransaction
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningRequestBody
import kotlinx.coroutines.withContext

class SignVrfInteractor @AssistedInject constructor(
    @Assisted private val body: SigningRequestBody.SignVrf,
    @Assisted private val signingSource: ProductSigningSource,
    private val coroutineDispatchers: CoroutineDispatchers,
) : TransactionSignInteractor {
    @AssistedFactory
    interface Factory {
        fun create(body: SigningRequestBody.SignVrf, signingSource: ProductSigningSource): SignVrfInteractor
    }

    override val account get() = signingSource.resolveAccount()

    override suspend fun parseSigningContent(): Result<ParsedSigningContent> = runCatching {
        ParsedSigningContent.Vrf(transcriptLabel = body.transcriptLabel.asReadableText())
    }

    override suspend fun humanReadableRepresentation(): Result<String> = runCatching {
        val transcript = buildList {
            add(TRANSCRIPT_LABEL_KEY to body.transcriptLabel.asReadableText())

            body.items.forEach { item ->
                add(item.label.value.asReadableText() to item.value.value.toHexString(withPrefix = true))
            }
        }

        transcript.joinToString(separator = "\n") { (key, content) -> "$key: $content" }
    }

    override suspend fun sign(): Result<SignedTransaction.Vrf> {
        return withContext(coroutineDispatchers.io) {
            signingSource.signVrf(body.transcriptLabel, body.items)
                .map { signature -> SignedTransaction.Vrf(signature = signature) }
        }
    }

    // Transcript labels are domain-separation strings in practice, but the wire type is opaque bytes.
    private fun ByteArray.asReadableText(): String {
        return runCatching { decodeToString(throwOnInvalidSequence = true) }
            .getOrElse { toHexString(withPrefix = true) }
    }

    private companion object {
        // Names the root `Transcript::new` argument alongside the item labels, which come off the wire.
        const val TRANSCRIPT_LABEL_KEY = "label"
    }
}
