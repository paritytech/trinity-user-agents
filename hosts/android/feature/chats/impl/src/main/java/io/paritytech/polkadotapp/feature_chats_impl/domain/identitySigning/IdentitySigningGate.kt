package io.paritytech.polkadotapp.feature_chats_impl.domain.identitySigning

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.IdentitySigningConsentRepository
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.withTimeoutOrNull
import timber.log.Timber
import javax.inject.Inject
import kotlin.time.Duration

/**
 * chat-spec RFC-0005 §4: when no anonymous slot is free, a chat's request, acceptance or messages may be signed by the
 * identity account only after the user allowed it for that chat. Checked before every such signature.
 */
class IdentitySigningGate @Inject constructor(
    private val consentRepository: IdentitySigningConsentRepository,
) {
    /**
     * Succeeds when the user allowed the identity signer for this chat. Otherwise asks for it (unless the user already
     * declined) and fails with [IdentitySigningConsentRequired] or [IdentitySigningDeclined].
     */
    suspend fun ensureConsent(contactAccountId: AccountId): Result<Unit> = runCancellableCatching {
        when (consentRepository.get(contactAccountId)) {
            IdentitySigningConsent.GRANTED -> Unit

            IdentitySigningConsent.DECLINED -> throw IdentitySigningDeclined(contactAccountId)

            IdentitySigningConsent.REQUESTED -> throw IdentitySigningConsentRequired(contactAccountId)

            null -> {
                Timber.i("identitySigning: asking the user to allow the identity signer for $contactAccountId")
                consentRepository.set(contactAccountId, IdentitySigningConsent.REQUESTED)
                throw IdentitySigningConsentRequired(contactAccountId)
            }
        }
    }

    suspend fun hasConsent(contactAccountId: AccountId): Boolean {
        return consentRepository.get(contactAccountId) == IdentitySigningConsent.GRANTED
    }

    /** Waits up to [timeout] for the consent of this chat to change, so a waiting signer reacts to the user at once. */
    suspend fun awaitConsentChange(contactAccountId: AccountId, timeout: Duration) {
        withTimeoutOrNull(timeout) {
            consentRepository.subscribe(contactAccountId)
                .distinctUntilChanged()
                .drop(1)
                .first()
        }
    }
}

/** Backs off [retryIn] after [error], returning early when the error was a pending consent and the user answered. */
suspend fun IdentitySigningGate.awaitRetry(contactAccountId: AccountId, error: Throwable?, retryIn: Duration) {
    if (error?.isAwaitingIdentitySigningConsent() == true) {
        awaitConsentChange(contactAccountId, retryIn)
    } else {
        delay(retryIn)
    }
}

fun Throwable.isAwaitingIdentitySigningConsent(): Boolean {
    return this is IdentitySigningConsentRequired || this is IdentitySigningDeclined
}
