package io.paritytech.polkadotapp.feature_transactions_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import dagger.multibindings.Multibinds
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.AsyncDurableSubmissionPolicy
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTransactionService
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.PinnedChainViewFactory
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.TxCompletionOracle
import io.paritytech.polkadotapp.feature_transactions_impl.data.durable.DurableTxRepository
import io.paritytech.polkadotapp.feature_transactions_impl.data.durable.RealDurableTxRepository
import io.paritytech.polkadotapp.feature_transactions_impl.data.durable.RealPinnedChainViewFactory
import io.paritytech.polkadotapp.feature_transactions_impl.domain.durable.DurableRecoveryPass
import io.paritytech.polkadotapp.feature_transactions_impl.domain.durable.DurableRecoveryScheduler
import io.paritytech.polkadotapp.feature_transactions_impl.domain.durable.RealDurableRecoveryPass
import io.paritytech.polkadotapp.feature_transactions_impl.domain.durable.RealDurableTransactionService
import io.paritytech.polkadotapp.feature_transactions_impl.domain.durable.WorkManagerDurableRecoveryScheduler
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
interface DurableTransactionModule {
    /**
     * Declared empty so the engine builds with no domains bound at all — every transaction is then decided
     * by the recorded-inclusion rule and the block search, which is correct and merely slower.
     */
    @Multibinds
    fun completionOracles(): Map<String, TxCompletionOracle>

    /** Declared empty so the engine builds with no policies bound — every failure is then terminal. */
    @Multibinds
    fun submissionPolicies(): Map<String, AsyncDurableSubmissionPolicy>

    @Binds
    @Singleton
    fun bindDurableTransactionService(impl: RealDurableTransactionService): DurableTransactionService

    @Binds
    fun bindDurableTxRepository(impl: RealDurableTxRepository): DurableTxRepository

    @Binds
    fun bindPinnedChainViewFactory(impl: RealPinnedChainViewFactory): PinnedChainViewFactory

    @Binds
    @Singleton
    fun bindDurableRecoveryPass(impl: RealDurableRecoveryPass): DurableRecoveryPass

    @Binds
    fun bindDurableRecoveryScheduler(impl: WorkManagerDurableRecoveryScheduler): DurableRecoveryScheduler
}
