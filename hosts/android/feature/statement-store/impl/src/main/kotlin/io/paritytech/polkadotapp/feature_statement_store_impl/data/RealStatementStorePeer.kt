package io.paritytech.polkadotapp.feature_statement_store_impl.data

import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.feature_statement_store_api.domain.StatementStorePeer
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onCompletion
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.transformLatest
import kotlinx.coroutines.flow.update
import javax.inject.Inject
import javax.inject.Singleton
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds

private val HANDOVER_GRACE: Duration = 2.seconds

@OptIn(ExperimentalCoroutinesApi::class)
@Singleton
class RealStatementStorePeer internal constructor(
    knownChains: KnownChains,
    private val handoverGrace: Duration,
) : StatementStorePeer {
    @Inject
    constructor(knownChains: KnownChains) : this(knownChains, HANDOVER_GRACE)

    override val chainId: ChainId = knownChains.people

    private val answering = MutableStateFlow(0)

    override fun observeAnswered(): Flow<Boolean> = flow {
        var everAnswered = false

        emitAll(
            answering
                .map { it > 0 }
                .distinctUntilChanged()
                .transformLatest { answered ->
                    // Holding the first reading would stall every source combined with it, so only a loss waits.
                    if (answered) everAnswered = true else if (everAnswered) delay(handoverGrace)

                    emit(answered)
                }
                .distinctUntilChanged(),
        )
    }

    fun <T> track(pages: Flow<Result<T>>): Flow<Result<T>> {
        var counted = false

        return pages
            .onEach { page ->
                if (!counted && page.isSuccess) {
                    counted = true
                    answering.update { it + 1 }
                }
            }
            .onCompletion { if (counted) answering.update { it - 1 } }
    }
}
