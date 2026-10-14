package io.paritytech.polkadotapp.feature_usernames_impl.domain.interactor

import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.flatRecover
import io.paritytech.polkadotapp.common.utils.mapError
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.storage.newaccount.NewAccountStorage
import io.paritytech.polkadotapp.feature_backup_api.domain.usecase.TryRecoverFromBackupAndCreateAccountUseCase
import io.paritytech.polkadotapp.feature_usernames_api.data.LocalUsernameStorage
import io.paritytech.polkadotapp.feature_usernames_api.domain.model.AccountOnboardingStatus
import io.paritytech.polkadotapp.feature_usernames_api.domain.model.Username
import io.paritytech.polkadotapp.feature_usernames_api.domain.usecase.ObserveAccountOnboardingStatusUseCase
import io.paritytech.polkadotapp.feature_usernames_api.domain.usecase.RecoverUsernameUseCase
import io.paritytech.polkadotapp.feature_usernames_impl.data.claim.UsernameAlreadyClaimedException
import io.paritytech.polkadotapp.feature_usernames_impl.data.claim.UsernameRepository
import io.paritytech.polkadotapp.feature_usernames_impl.data.storage.QueuedClaimStorage
import io.paritytech.polkadotapp.feature_usernames_impl.domain.error.UsernameFlowError
import io.paritytech.polkadotapp.feature_usernames_impl.domain.error.asUsernameFlowError
import io.paritytech.polkadotapp.feature_usernames_impl.domain.error.toUsernameFlowError
import io.paritytech.polkadotapp.feature_usernames_impl.domain.model.ClaimUsernameOutcome
import io.paritytech.polkadotapp.feature_usernames_impl.domain.model.UsernameAvailabilityState
import io.paritytech.polkadotapp.feature_usernames_impl.domain.model.UsernameClaimResult
import io.paritytech.polkadotapp.feature_usernames_impl.domain.usecase.CreateClaimParamsUseCase
import io.paritytech.polkadotapp.tools_backup_api.domain.model.BackupOutcome
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.withContext
import javax.inject.Inject

interface UsernamesClaimInteractor {
    suspend fun checkUsernameAvailable(username: Username): Result<UsernameAvailabilityState>

    suspend fun claimUsername(username: Username, preferredDigits: String): ClaimUsernameOutcome

    suspend fun tryRecoverBackupOrCreateAccount(): Result<BackupOutcome>

    suspend fun areAccountsInitialized(): Boolean

    fun observeAccountOnboardingStatus(): Flow<AccountOnboardingStatus>

    suspend fun recoverUsername(): Result<Boolean>

    suspend fun saveIsNewAccount()
}

class RealUsernamesClaimInteractor @Inject constructor(
    private val newAccountStorage: NewAccountStorage,
    private val usernameRepository: UsernameRepository,
    private val coroutineDispatchers: CoroutineDispatchers,
    private val localUsernameStorage: LocalUsernameStorage,
    private val queuedClaimStorage: QueuedClaimStorage,
    private val createClaimParamsUseCase: CreateClaimParamsUseCase,
    private val observeAccountOnboardingStatusUseCase: ObserveAccountOnboardingStatusUseCase,
    private val tryRecoverFromBackupAndCreateAccountUseCase: TryRecoverFromBackupAndCreateAccountUseCase,
    private val accountRepository: AccountRepository,
    private val recoverUsernameUseCase: RecoverUsernameUseCase,
) : UsernamesClaimInteractor {
    override suspend fun tryRecoverBackupOrCreateAccount(): Result<BackupOutcome> {
        return tryRecoverFromBackupAndCreateAccountUseCase().mapError(Throwable::toUsernameFlowError)
    }

    override suspend fun checkUsernameAvailable(username: Username): Result<UsernameAvailabilityState> {
        return availabilityOf(username).flatRecover { error ->
            // Attestation blips clear on the next challenge, so spend exactly one more attempt
            // before telling the user anything.
            if (error == UsernameFlowError.VerificationBusy) {
                availabilityOf(username)
            } else {
                Result.failure(error)
            }
        }
    }

    private suspend fun availabilityOf(username: Username): Result<UsernameAvailabilityState> {
        return usernameRepository.checkUsernameAvailable(username).mapError(Throwable::toUsernameFlowError)
    }

    override suspend fun claimUsername(username: Username, preferredDigits: String): ClaimUsernameOutcome {
        return withContext(coroutineDispatchers.io) {
            usernameRepository.getVerifier()
                .flatMap { createClaimParamsUseCase(username, it, preferredDigits) }
                .flatMap { usernameRepository.claimUsername(it) }
                .fold(
                    onSuccess = { handleClaimResult(it) },
                    onFailure = { mapClaimFailure(it, username) }
                )
        }
    }

    private suspend fun handleClaimResult(result: UsernameClaimResult): ClaimUsernameOutcome {
        return when (result) {
            is UsernameClaimResult.Registered -> {
                localUsernameStorage.saveValue(result.username)
                ClaimUsernameOutcome.Claimed
            }

            is UsernameClaimResult.Queued -> {
                queuedClaimStorage.saveValue(result.username)
                ClaimUsernameOutcome.Queued
            }

            UsernameClaimResult.PaymentRequired -> ClaimUsernameOutcome.PaymentRequired
        }
    }

    private suspend fun mapClaimFailure(error: Throwable, username: Username): ClaimUsernameOutcome {
        return if (error is UsernameAlreadyClaimedException) {
            recoverFromConflict(username)
        } else {
            ClaimUsernameOutcome.Failed(error.asUsernameFlowError())
        }
    }

    private suspend fun recoverFromConflict(username: Username): ClaimUsernameOutcome {
        return usernameRepository.checkUsernameAvailable(username).fold(
            onSuccess = { availability ->
                when (availability) {
                    is UsernameAvailabilityState.Available -> ClaimUsernameOutcome.SuffixTaken(availability.availableDigits)
                    UsernameAvailabilityState.Taken,
                    UsernameAvailabilityState.Invalid -> ClaimUsernameOutcome.Unavailable
                }
            },
            onFailure = { ClaimUsernameOutcome.Failed(it.asUsernameFlowError()) }
        )
    }

    override suspend fun areAccountsInitialized(): Boolean {
        return accountRepository.areAccountsInitialized()
    }

    override fun observeAccountOnboardingStatus() = observeAccountOnboardingStatusUseCase()

    override suspend fun recoverUsername() = recoverUsernameUseCase().mapError(Throwable::toUsernameFlowError)

    override suspend fun saveIsNewAccount() {
        newAccountStorage.saveValue(true)
    }
}
