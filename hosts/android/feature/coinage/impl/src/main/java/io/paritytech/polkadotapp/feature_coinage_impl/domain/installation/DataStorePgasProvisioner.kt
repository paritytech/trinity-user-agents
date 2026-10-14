package io.paritytech.polkadotapp.feature_coinage_impl.domain.installation

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.progressStallReport.StalenessReportCollector
import io.paritytech.polkadotapp.feature_balances_api.data.type.TokenBalanceTypeRegistry
import io.paritytech.polkadotapp.feature_coinage_impl.domain.coinageLogI
import io.paritytech.polkadotapp.feature_coinage_impl.domain.coinageLogW
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCheckMemberInRingUseCase
import io.paritytech.polkadotapp.feature_people_api.domain.useCase.ActivePeopleCollectionUseCase
import io.paritytech.polkadotapp.feature_pgas_api.domain.OnExistingAllocationStrategy
import io.paritytech.polkadotapp.feature_pgas_api.domain.PgasChainAssetProvider
import io.paritytech.polkadotapp.feature_pgas_api.domain.PgasClaimer
import kotlinx.coroutines.withTimeoutOrNull
import javax.inject.Inject
import kotlin.time.Duration.Companion.minutes

class DataStorePgasShortError(required: Balance, available: Balance) :
    Exception("Data store account holds $available PGAS after a top-up, $required required")

class DataStorePgasClaimTimeoutError : Exception("PGAS claim did not report back in time")

// The data store account pays both the fee and the storage deposit in PGAS. Claiming takes a ring proof, so a
// claim waits for this person to be included in the ring first — a fresh account simply waits here.
class DataStorePgasProvisioner @Inject constructor(
    private val activePeopleCollectionUseCase: ActivePeopleCollectionUseCase,
    private val peopleCheckMemberInRingUseCase: PeopleCheckMemberInRingUseCase,
    private val pgasClaimer: PgasClaimer,
    private val pgasChainAssetProvider: PgasChainAssetProvider,
    private val tokenBalanceTypeRegistry: TokenBalanceTypeRegistry,
) {
    // An empty account cannot even be simulated against: the dry-run fails on the deposit it cannot reserve.
    suspend fun ensureFunded(account: AccountId): Result<Unit> {
        return pgasBalance(account).flatMap { transferable ->
            if (transferable > Balance.ZERO) {
                coinageLogI("Installation registration: PGAS funding skipped, data store account holds $transferable")
                Result.success(Unit)
            } else {
                coinageLogI("Installation registration: data store account is empty, claiming PGAS")
                claim(account, OnExistingAllocationStrategy.IGNORE)
                    .onSuccess { coinageLogI("Installation registration: PGAS claimed for the data store account") }
            }
        }
    }

    suspend fun ensureCovers(account: AccountId, required: Balance): Result<Unit> {
        return pgasBalance(account).flatMap { transferable ->
            if (transferable >= required) {
                coinageLogI("Installation registration: PGAS covers the call, holds $transferable, needs $required")
                Result.success(Unit)
            } else {
                coinageLogI("Installation registration: PGAS short, holds $transferable, needs $required, topping up")
                claim(account, OnExistingAllocationStrategy.INCREASE)
                    .flatMap { pgasBalance(account) }
                    .flatMap { topped ->
                        if (topped >= required) {
                            coinageLogI("Installation registration: PGAS topped up to $topped")
                            Result.success(Unit)
                        } else {
                            Result.failure(DataStorePgasShortError(required, topped))
                        }
                    }
            }
        }
    }

    private suspend fun claim(account: AccountId, strategy: OnExistingAllocationStrategy): Result<Unit> {
        val collection = activePeopleCollectionUseCase.getActivePeopleCollection()

        return peopleCheckMemberInRingUseCase.awaitIncluded(collection).flatMap {
            // The claim waits for inclusion with no bound of its own, and a status stream that goes quiet would hold
            // the registration forever. Giving up is safe: a retry reads the balance first, and a claim for the same
            // slot proves the same alias, which the chain accepts once.
            withTimeoutOrNull(CLAIM_TIMEOUT) {
                with(StalenessReportCollector.NoOp) { pgasClaimer.claim(account, strategy) }
            } ?: run {
                coinageLogW("Installation registration: PGAS claim did not report back within $CLAIM_TIMEOUT")
                Result.failure(DataStorePgasClaimTimeoutError())
            }
        }
    }

    private companion object {
        val CLAIM_TIMEOUT = 2.minutes
    }

    private suspend fun pgasBalance(account: AccountId): Result<Balance> = runCatching {
        tokenBalanceTypeRegistry.typeFor(pgasChainAssetProvider.asset()).getBalance(account).transferable
    }
}
