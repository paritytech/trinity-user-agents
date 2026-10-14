package io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling

import io.paritytech.polkadotapp.common.data.time.TimeProvider
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.RecyclingStrategyType
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.params
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.TrackedVoucher
import io.paritytech.polkadotapp.feature_coinage_impl.data.source.ClockChangesSource
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.buffer
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.onStart
import javax.inject.Inject

/** Keeps readiness timers tied to the latest wallet state and strategy. */
class VoucherReadinessUpdates @Inject constructor(
    private val timeProvider: TimeProvider,
    private val clockChangesSource: ClockChangesSource,
) {
    @OptIn(ExperimentalCoroutinesApi::class)
    internal fun <Input> observe(
        inputs: Flow<Input>,
        vouchers: (Input) -> List<TrackedVoucher>,
        strategy: (Input) -> RecyclingStrategyType,
    ): Flow<Input> = inputs.combine(clockChangesSource.changes().onStart { emit(Unit) }) { input, _ -> input }
        .flatMapLatest { input ->
            flow {
                val now = timeProvider.now()
                val readiness = strategy(input).params.voucherReadiness.memberAndAgeRequirements
                val deadlines = if (readiness == null) emptyList() else vouchers(input).mapNotNull { tracked ->
                    if (!tracked.state.isFree) return@mapNotNull null
                    val location = tracked.voucher.location as? RecyclerVoucher.Location.InRecycler
                    val enteredAt = location?.enteredAt
                    if (enteredAt != null && location.recyclerMembers >= readiness.minimumMembers) {
                        (enteredAt + readiness.delay).takeIf { it > now }
                    } else {
                        null
                    }
                }.distinct().sorted()

                emit(input)
                for (deadline in deadlines) {
                    while (timeProvider.now() < deadline) {
                        delay(deadline - timeProvider.now())
                    }
                    emit(input)
                }
            }
        }.buffer(0)
}

/** Re-emits the latest input when voucher readiness may change. */
fun <Input> Flow<Input>.withReadinessUpdates(
    readinessUpdates: VoucherReadinessUpdates,
    vouchers: (Input) -> List<TrackedVoucher>,
    strategy: (Input) -> RecyclingStrategyType,
): Flow<Input> = readinessUpdates.observe(this, vouchers, strategy)
