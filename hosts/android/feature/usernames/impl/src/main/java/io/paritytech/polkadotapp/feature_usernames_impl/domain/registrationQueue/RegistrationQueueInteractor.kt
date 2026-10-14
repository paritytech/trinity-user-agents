package io.paritytech.polkadotapp.feature_usernames_impl.domain.registrationQueue

import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_usernames_api.data.LocalUsernameStorage
import io.paritytech.polkadotapp.feature_usernames_impl.data.claim.QueueRateLimitedException
import io.paritytech.polkadotapp.feature_usernames_impl.data.claim.UsernameRepository
import io.paritytech.polkadotapp.feature_usernames_impl.data.storage.QueuedClaimStorage
import io.paritytech.polkadotapp.feature_usernames_impl.domain.model.RegistrationQueueStatus
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import javax.inject.Inject
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds

class RegistrationQueueInteractor @Inject constructor(
    private val usernameRepository: UsernameRepository,
    private val localUsernameStorage: LocalUsernameStorage,
    private val queuedClaimStorage: QueuedClaimStorage
) {
    fun observeQueueProgress(): Flow<RegistrationQueueProgress> = flow {
        var initialPosition: Int? = null
        var backoffDelay = POLL_INTERVAL

        while (true) {
            val result = usernameRepository.getRegistrationQueueStatus().logFailure("RegistrationQueue")
            when (val status = result.getOrNull()) {
                is RegistrationQueueStatus.InQueue -> {
                    backoffDelay = POLL_INTERVAL
                    val initial = initialPosition ?: status.position.also { initialPosition = it }
                    emit(RegistrationQueueProgress.Waiting(position = status.position, initialPosition = initial))
                    delay(POLL_INTERVAL)
                }

                RegistrationQueueStatus.NotQueued -> {
                    completeRegistration()
                    emit(RegistrationQueueProgress.Completed)
                    return@flow
                }

                null -> {
                    delay(retryDelay(result.exceptionOrNull(), backoffDelay))
                    backoffDelay = (backoffDelay * 2).coerceAtMost(MAX_BACKOFF_DELAY)
                }
            }
        }
    }

    private suspend fun completeRegistration() {
        queuedClaimStorage.getValue()?.let { localUsernameStorage.saveValue(it) }
        queuedClaimStorage.removeValue()
    }

    private fun retryDelay(error: Throwable?, backoffDelay: Duration): Duration {
        val retryAfter = (error as? QueueRateLimitedException)?.retryAfterSeconds?.seconds
        return retryAfter ?: backoffDelay
    }

    companion object {
        internal val POLL_INTERVAL = 10.seconds
        internal val MAX_BACKOFF_DELAY = 60.seconds
    }
}
