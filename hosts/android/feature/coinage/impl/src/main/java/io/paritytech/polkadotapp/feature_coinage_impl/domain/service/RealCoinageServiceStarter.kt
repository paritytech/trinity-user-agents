package io.paritytech.polkadotapp.feature_coinage_impl.domain.service

import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_coinage_api.domain.service.CoinageBackupService
import io.paritytech.polkadotapp.feature_coinage_api.domain.service.CoinageServiceStarter
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.CoinageTransactionService
import io.paritytech.polkadotapp.feature_coinage_impl.domain.installation.CoinageInstallationRegistrar
import io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling.CoinRecyclingEvaluator
import io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.CoinageHandoffGuard
import io.paritytech.polkadotapp.feature_usernames_api.domain.usecase.ObserveAccountOnboardingStatusUseCase
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import javax.inject.Inject

class RealCoinageServiceStarter @Inject constructor(
    private val coinageBackupService: CoinageBackupService,
    private val voucherLocationService: VoucherLocationService,
    private val coinPresenceSyncService: CoinPresenceSyncService,
    private val coinRecyclingEvaluator: CoinRecyclingEvaluator,
    private val observeAccountOnboardingStatusUseCase: ObserveAccountOnboardingStatusUseCase,
    private val coinageTransactionService: CoinageTransactionService,
    private val installationRegistrar: CoinageInstallationRegistrar,
    private val handoffGuard: CoinageHandoffGuard,
) : CoinageServiceStarter {
    context(scope: ComputationalScope)
    override fun start() {
        scope.launch { coinPresenceSyncService.start() }
        scope.launch { voucherLocationService.start() }
        coinRecyclingEvaluator.start()
        installationRegistrar.start()
        scope.launch {
            observeAccountOnboardingStatusUseCase().filter { it.isOnboarded }.first()
            coinageBackupService.start()
        }
        scope.launch {
            // A reservation that never became a payment: its keys never left, so the assets come back.
            // Before recovery, so nothing decides an entry against a mark that is about to disappear.
            // The guard keeps it to a real process start: this runs again on every Activity recreation, and
            // clearing then would drop the mark of a send that is already handing its keys to a peer.
            if (handoffGuard.claimStartupRelease()) {
                coinageTransactionService.releaseUncommittedHandoffs()
                    .logFailure("Failed to release uncommitted coinage handoffs")
            }

            // Entries left live by a previous process are decided from the chain, not resumed.
            coinageTransactionService.startRecovery()
        }
    }
}
