package io.paritytech.polkadotapp.feature_usernames_impl.presentation.claim

import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.feature_backup_api.presentation.BackupFoundPayload
import io.paritytech.polkadotapp.feature_usernames_api.domain.model.AccountOnboardingStatus
import io.paritytech.polkadotapp.feature_usernames_api.domain.model.Username
import io.paritytech.polkadotapp.feature_usernames_api.presentation.MIN_USERNAME_LENGTH
import io.paritytech.polkadotapp.feature_usernames_api.presentation.model.DigitsFieldState
import io.paritytech.polkadotapp.feature_usernames_impl.domain.error.UsernameFlowError
import io.paritytech.polkadotapp.feature_usernames_impl.domain.error.asUsernameFlowError
import io.paritytech.polkadotapp.feature_usernames_impl.domain.interactor.UsernamesClaimInteractor
import io.paritytech.polkadotapp.feature_usernames_impl.domain.model.ClaimUsernameOutcome
import io.paritytech.polkadotapp.feature_usernames_impl.domain.model.UsernameAvailabilityState
import io.paritytech.polkadotapp.feature_usernames_impl.presentation.UsernamesRouter
import io.paritytech.polkadotapp.tools_backup_api.domain.model.BackupOutcome
import kotlinx.collections.immutable.persistentListOf
import kotlinx.collections.immutable.toImmutableList
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.mapLatest
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import timber.log.Timber
import javax.inject.Inject
import kotlin.time.Duration.Companion.milliseconds
import io.paritytech.polkadotapp.common.R as RCommon

private const val MAX_USERNAME_LENGTH = 29
private const val MAX_DIGITS_LENGTH = 2

@HiltViewModel
class ClaimUsernameViewModel @Inject constructor(
    private val router: UsernamesRouter,
    private val interactor: UsernamesClaimInteractor,
) : BaseViewModel(), ClaimUsernameContract {
    override val state = MutableStateFlow(ClaimUsernameState())

    private val _messageEvents = MutableSharedFlow<Int>(extraBufferCapacity = 1)
    override val messageEvents: SharedFlow<Int> = _messageEvents

    init {
        observeUsernameChanges()
        // All the logic is concentrated in this method,
        // which observes the chain and local storage for when an account
        // has been created and the username has been claimed
        observeOnboardingState()
    }

    private fun observeOnboardingState() {
        interactor.observeAccountOnboardingStatus()
            .onEach { status -> handleOnboardingStatus(status) }
            .launchIn(this)
    }

    private suspend fun handleOnboardingStatus(status: AccountOnboardingStatus) {
        when {
            status.isOnboarded -> router.openMain()
            status.isWaitingInQueue -> router.openRegistrationQueue()
            status.accountCreated -> {
                state.update { it.copy(showRecoverOption = false) }
                tryRecoverUsername()
            }

            else -> {
                state.update { it.copy(showRecoverOption = true) }
            }
        }
    }

    private suspend fun tryRecoverUsername() {
        interactor.recoverUsername()
            .onSuccess { usernameFound ->
                if (!usernameFound) {
                    interactor.saveIsNewAccount()
                }
            }
            .onFailure { error -> applyFieldError(error) }
    }

    private fun meetsMinimumLength(name: String) = name.length >= MIN_USERNAME_LENGTH

    private fun observeUsernameChanges() {
        state.map { it.username }
            .distinctUntilChanged()
            .debounce(300.milliseconds)
            .filter(::meetsMinimumLength)
            .mapLatest { username ->
                state.update {
                    it.copy(
                        fieldState = ClaimUsernameFieldState.Neutral,
                        availableDigits = persistentListOf(),
                        digitsFieldState = DigitsFieldState.Hidden
                    )
                }
                interactor.checkUsernameAvailable(Username.fromParts(username, null))
                    .onSuccess { availabilityState ->
                        when (availabilityState) {
                            is UsernameAvailabilityState.Available -> {
                                val firstDigits = availabilityState.availableDigits.firstOrNull().orEmpty()
                                state.update {
                                    it.copy(
                                        fieldState = ClaimUsernameFieldState.Available,
                                        availableDigits = availabilityState.availableDigits.toImmutableList(),
                                        digitsFieldState = if (firstDigits.isNotEmpty()) {
                                            DigitsFieldState.Visible(digits = firstDigits, isValid = true)
                                        } else {
                                            DigitsFieldState.Hidden
                                        }
                                    )
                                }
                            }

                            is UsernameAvailabilityState.Taken -> {
                                state.update { it.copy(fieldState = ClaimUsernameFieldState.Taken) }
                            }

                            is UsernameAvailabilityState.Invalid -> {
                                state.update { it.copy(fieldState = ClaimUsernameFieldState.Invalid) }
                            }
                        }
                    }
                    .onFailure { error -> applyFieldError(error) }
            }
            .launchIn(this)
    }

    override fun backPressed() {
        router.back()
    }

    override fun onUsernameChanged(value: String) {
        val previousValue = state.value.username
        val newValue = value.filterUsernameInput()

        if (previousValue != newValue) {
            state.update {
                it.copy(
                    username = newValue,
                    fieldState = ClaimUsernameFieldState.Neutral,
                    availableDigits = persistentListOf(),
                    digitsFieldState = DigitsFieldState.Hidden
                )
            }
        }
    }

    /** Validates entered digits client-side against the stored [ClaimUsernameState.availableDigits] list. */
    override fun onDigitsChanged(value: String) {
        val filtered = value.filter { it.isDigit() }.take(MAX_DIGITS_LENGTH)
        val availableDigits = state.value.availableDigits
        val isValid = filtered.length == MAX_DIGITS_LENGTH && filtered in availableDigits
        state.update {
            it.copy(digitsFieldState = DigitsFieldState.Visible(digits = filtered, isValid = isValid))
        }
    }

    override fun onClaimClicked() {
        launch {
            if (interactor.areAccountsInitialized()) {
                claimUsername(ClaimUsernameProgress.CREATING)
            } else {
                state.update { it.copy(progress = ClaimUsernameProgress.CLAIMING) }

                interactor.tryRecoverBackupOrCreateAccount()
                    .onSuccess { handleOutcome(it) }
                    .onFailure { error -> applyFieldError(error) }
            }
        }
    }

    private fun handleOutcome(outcome: BackupOutcome) {
        when (outcome) {
            BackupOutcome.Created,
            BackupOutcome.AccountsCreatedButBackupFailed,
            BackupOutcome.NoNeedToBackup -> claimUsername(ClaimUsernameProgress.CREATING)

            is BackupOutcome.ExistingBackupFound -> {
                state.update { it.copy(progress = ClaimUsernameProgress.NONE) }
                router.openBackupFound(
                    BackupFoundPayload(outcome.createdAt, outcome.accountId.value)
                )
            }
        }
    }

    private fun claimUsername(progress: ClaimUsernameProgress) {
        state.update { it.copy(progress = progress) }

        launch {
            val baseUsername = Username.fromParts(state.value.username, null)
            val preferredDigits = (state.value.digitsFieldState as? DigitsFieldState.Visible)?.digits.orEmpty()

            interactor.checkUsernameAvailable(baseUsername)
                .onSuccess { availability ->
                    if (availability is UsernameAvailabilityState.Available) {
                        handleClaimOutcome(interactor.claimUsername(baseUsername, preferredDigits))
                    } else {
                        state.update {
                            it.copy(progress = ClaimUsernameProgress.NONE, fieldState = ClaimUsernameFieldState.Taken)
                        }
                    }
                }
                // A failed re-check used to render as "taken", blaming the user for a network blip.
                .onFailure { error -> applyFieldError(error) }
        }
    }

    private suspend fun handleClaimOutcome(outcome: ClaimUsernameOutcome) {
        when (outcome) {
            ClaimUsernameOutcome.Claimed -> Unit
            ClaimUsernameOutcome.Queued -> Unit
            ClaimUsernameOutcome.PaymentRequired -> {
                state.update { it.copy(progress = ClaimUsernameProgress.NONE) }
                router.openClaimUnavailable()
            }

            is ClaimUsernameOutcome.SuffixTaken -> {
                val firstDigits = outcome.freshDigits.firstOrNull().orEmpty()
                state.update {
                    it.copy(
                        progress = ClaimUsernameProgress.NONE,
                        fieldState = ClaimUsernameFieldState.Available,
                        availableDigits = outcome.freshDigits.toImmutableList(),
                        digitsFieldState = if (firstDigits.isNotEmpty()) {
                            DigitsFieldState.Visible(digits = firstDigits, isValid = true)
                        } else {
                            DigitsFieldState.Hidden
                        }
                    )
                }
                _messageEvents.emit(RCommon.string.pick_username_just_claimed_new_digit)
            }

            ClaimUsernameOutcome.Unavailable -> {
                state.update {
                    it.copy(progress = ClaimUsernameProgress.NONE, fieldState = ClaimUsernameFieldState.Taken)
                }
                _messageEvents.emit(RCommon.string.pick_username_just_claimed_taken)
            }

            is ClaimUsernameOutcome.Failed -> {
                state.update { it.copy(progress = ClaimUsernameProgress.NONE) }
                handleClaimError(outcome.error)
            }
        }
    }

    private fun handleClaimError(error: UsernameFlowError) {
        when (error) {
            UsernameFlowError.VerificationUnavailable -> router.openClaimUnavailable()
            UsernameFlowError.VerificationRejected -> router.openIntegrityFailed()

            else -> state.update { it.copy(fieldState = ClaimUsernameFieldState.Error(error)) }
        }
    }

    private fun applyFieldError(error: Throwable) {
        val flowError = error.asUsernameFlowError()

        // Backing out is not a failure to display, mirroring BaseViewModel.shouldIgnore.
        if (flowError == UsernameFlowError.Cancelled) {
            state.update { it.copy(progress = ClaimUsernameProgress.NONE) }
            return
        }

        // The variant goes in the message: payload-free errors override fillInStackTrace, so
        // passing one as the throwable alone prints no identity at all.
        Timber.w(error, "Claim username flow failed: %s", flowError)
        state.update {
            it.copy(
                progress = ClaimUsernameProgress.NONE,
                fieldState = ClaimUsernameFieldState.Error(flowError),
            )
        }
    }

    override fun onClearAction() {
        state.update {
            it.copy(
                username = "",
                fieldState = ClaimUsernameFieldState.Neutral,
                availableDigits = persistentListOf(),
                digitsFieldState = DigitsFieldState.Hidden
            )
        }
    }

    override fun onRecoverClicked() {
        if (state.value.progress != ClaimUsernameProgress.NONE) return

        router.openRecoverOptions()
    }

    override fun onTermsClicked() {
        router.openTermsOfUse()
    }

    override fun onPrivacyPolicyClicked() {
        router.openPrivacyPolicy()
    }

    override fun onBackupOverridden() {
        if (meetsMinimumLength(state.value.username)) {
            claimUsername(ClaimUsernameProgress.CREATING)
        }
    }

    override fun onImportedFromBackup() {
        state.update { it.copy(progress = ClaimUsernameProgress.RECOVERING) }
    }
}

fun String.filterUsernameInput(): String = filter { it.isLetter() }
    .take(MAX_USERNAME_LENGTH)
    .lowercase()
