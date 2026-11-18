package io.paritytech.polkadotapp.feature_coinage_impl.core

import io.novasama.substrate_sdk_android.encrypt.Sr25519
import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.Sr25519Keypair
import io.paritytech.polkadotapp.chains.util.planksFromAmount
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageTransferDetection
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageOperationGroupId
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.OnboardingUseCase
import io.paritytech.polkadotapp.feature_tokens_api.di.DigitalDollarChainAssetProvider
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionSignerSource
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flow
import uniffi.truapi.NativeFunding
import uniffi.truapi.Retry
import java.math.BigDecimal
import javax.inject.Inject
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

class UnsupportedFundingSignerException : IllegalArgumentException("A top-up is paid only from an sr25519 keypair")

@OptIn(ExperimentalTime::class)
class CoreOnboardingUseCase @Inject constructor(
    private val core: CoinageCore,
    @param:DigitalDollarChainAssetProvider private val chainAssetProvider: ChainAssetProvider,
) : OnboardingUseCase {
    override suspend fun onboard(amount: BigDecimal, signerSource: TransactionSignerSource.Signed): Result<Unit> =
        runCancellableCatching { signerSource.funding() }.flatMap { funding ->
            core.units().flatMap { units ->
                val cents = units.centsOf(chainAssetProvider.asset().planksFromAmount(amount))
                core.use { coinage -> coinage.topUp(CoinageIds.random(), Retry.Never, cents, funding) }
            }
        }.map { }

    override fun onboardDurably(
        amount: BigDecimal,
        signerSource: TransactionSignerSource.Signed,
        groupId: CoinageOperationGroupId,
        retryUntil: Instant,
    ): Flow<CoinageTransferDetection> = flow {
        val funding = signerSource.funding()
        val units = core.units().getOrElse { throw it }
        val cents = units.centsOf(chainAssetProvider.asset().planksFromAmount(amount))
        val id = CoinageIds.of(groupId)
        emitAll(
            core.followOperation(id) { coinage ->
                coinage.topUp(id, Retry.Until(retryUntil.epochSeconds.toULong()), cents, funding)
            }
        )
    }
}

private fun TransactionSignerSource.Signed.funding(): NativeFunding {
    val keypair = (this as? TransactionSignerSource.FromKeyPair)?.keypair as? Sr25519Keypair
        ?: throw UnsupportedFundingSignerException()
    return KeypairFunding(keypair)
}

/** Pays a top-up from [keypair]'s account, signing every payload the core asks for. */
private class KeypairFunding(private val keypair: Sr25519Keypair) : NativeFunding {
    override fun account(): ByteArray = keypair.publicKey

    override fun sign(payload: ByteArray): ByteArray =
        Sr25519.sign(keypair.publicKey, keypair.privateKey + keypair.nonce, payload)
}
