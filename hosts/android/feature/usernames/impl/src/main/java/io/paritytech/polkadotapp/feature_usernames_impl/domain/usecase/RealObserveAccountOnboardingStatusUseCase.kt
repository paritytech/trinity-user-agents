package io.paritytech.polkadotapp.feature_usernames_impl.domain.usecase

import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_usernames_api.domain.model.AccountOnboardingStatus
import io.paritytech.polkadotapp.feature_usernames_api.domain.usecase.ObserveAccountOnboardingStatusUseCase
import io.paritytech.polkadotapp.feature_usernames_api.domain.usecase.UsernameOfAccountUseCase
import io.paritytech.polkadotapp.feature_usernames_impl.data.storage.QueuedClaimStorage
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import javax.inject.Inject

class RealObserveAccountOnboardingStatusUseCase @Inject constructor(
    private val accountRepository: AccountRepository,
    private val usernameOfAccountUseCase: UsernameOfAccountUseCase,
    private val queuedClaimStorage: QueuedClaimStorage,
) : ObserveAccountOnboardingStatusUseCase {
    override fun invoke(): Flow<AccountOnboardingStatus> =
        combine(
            accountRepository.areAccountsInitializedFlow(),
            usernameOfAccountUseCase.initiallyClaimedLightUsername(),
            queuedClaimStorage.valueFlow()
        ) { hasAccount, initiallyClaimedLightUsername, queuedUsername ->
            if (!hasAccount) return@combine AccountOnboardingStatus.EMPTY

            AccountOnboardingStatus(
                accountCreated = true,
                usernameClaimed = initiallyClaimedLightUsername,
                queuedUsername = queuedUsername
            )
        }
            .distinctUntilChanged()
}
