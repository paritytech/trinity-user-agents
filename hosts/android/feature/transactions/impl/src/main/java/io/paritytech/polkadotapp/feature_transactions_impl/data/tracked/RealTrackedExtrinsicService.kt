package io.paritytech.polkadotapp.feature_transactions_impl.data.tracked

import dagger.Lazy
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.feature_transactions.api.data.ExtrinsicService
import io.paritytech.polkadotapp.feature_transactions.api.data.FormExtrinsic
import io.paritytech.polkadotapp.feature_transactions.api.data.retry.PreSubmissionValidationFailed
import io.paritytech.polkadotapp.feature_transactions.api.data.tracked.ActiveTrackedExtrinsic
import io.paritytech.polkadotapp.feature_transactions.api.data.tracked.ExtrinsicTag
import io.paritytech.polkadotapp.feature_transactions.api.data.tracked.TrackedExtrinsicService
import io.paritytech.polkadotapp.feature_transactions.api.data.tracked.TrackedExtrinsicStatus
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionOrigin
import io.paritytech.polkadotapp.feature_transactions_impl.data.validation.PreSubmissionValidator
import kotlinx.coroutines.flow.Flow
import timber.log.Timber
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class RealTrackedExtrinsicService @Inject constructor(
    // Lazy: the override interceptor sits on the storage read path that ExtrinsicService transitively builds on,
    // so eager injection would risk a construction cycle (interceptor → this → ExtrinsicService → storage → …).
    private val extrinsicService: Lazy<ExtrinsicService>,
    private val preSubmissionValidator: PreSubmissionValidator,
    private val repository: TrackedExtrinsicRepository,
    private val enqueuer: TrackedExtrinsicEnqueuer,
) : TrackedExtrinsicService {
    override suspend fun submit(
        tag: ExtrinsicTag,
        chain: Chain,
        origin: TransactionOrigin,
        options: ExtrinsicService.SubmissionOptions,
        additional: DataByteArray?,
        formExtrinsic: FormExtrinsic,
    ): Result<Unit> {
        Timber.d("submit($tag): building + signing on ${chain.id}")

        return extrinsicService.get().buildExtrinsic(chain, origin, options, formExtrinsic).flatMap { extrinsic ->
            if (!preSubmissionValidator.mightBeValid(chain.id, extrinsic)) {
                return@flatMap Result.failure(PreSubmissionValidationFailed())
            }

            repository.save(
                tag = tag,
                chainId = chain.id,
                signedExtrinsic = extrinsic.extrinsicHex,
                additional = additional,
                createdAt = System.currentTimeMillis(),
            )
            Timber.d("submit($tag): persisted as Pending; enqueuing ${TrackedExtrinsicWorker.WORK_ID} worker")

            enqueuer.enqueue()
            Timber.d("submit($tag): worker enqueued")

            Result.success(Unit)
        }.onFailure { Timber.w(it, "submit($tag): not enqueued") }
    }

    override fun observeStatus(tag: ExtrinsicTag): Flow<TrackedExtrinsicStatus?> = repository.observeStatus(tag)

    override suspend fun getLatestActive(prefix: String): ActiveTrackedExtrinsic? = repository.getLatestActive(prefix)

    override fun observeLatestActive(prefix: String): Flow<ActiveTrackedExtrinsic?> = repository.observeLatestActive(prefix)
}
