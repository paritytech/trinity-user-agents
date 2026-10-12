package io.paritytech.polkadotapp.feature_videogame_impl.presentation.bot

import android.content.Context
import dagger.hilt.android.lifecycle.HiltViewModel
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainAssetWithAmount
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.calendar.CalendarEvent
import io.paritytech.polkadotapp.common.utils.calendar.CalendarEventsMixin
import io.paritytech.polkadotapp.common.utils.combine
import io.paritytech.polkadotapp.common.utils.disable
import io.paritytech.polkadotapp.common.utils.enable
import io.paritytech.polkadotapp.common.utils.flowOf
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.common.utils.shareInBackground
import io.paritytech.polkadotapp.common.utils.stateInBackground
import io.paritytech.polkadotapp.feature_people_api.presentation.mixin.DimSwitchMixin
import io.paritytech.polkadotapp.feature_people_api.presentation.mixin.create
import io.paritytech.polkadotapp.feature_tokens_api.presentation.mapper.TokenAmountMapper
import io.paritytech.polkadotapp.feature_upgrade_username_api.domain.model.UpgradeToFullUsernameState
import io.paritytech.polkadotapp.feature_upgrade_username_api.presentation.bot.toBotUi
import io.paritytech.polkadotapp.feature_videogame_impl.VideoGameRouter
import io.paritytech.polkadotapp.feature_videogame_impl.data.VideoGameInfoSyncService
import io.paritytech.polkadotapp.feature_videogame_impl.data.calendar.gameCalendarEventTitle
import io.paritytech.polkadotapp.feature_videogame_impl.data.models.gameDuration
import io.paritytech.polkadotapp.feature_videogame_impl.domain.dim.Dim2CommitmentHandler
import io.paritytech.polkadotapp.feature_videogame_impl.domain.interactor.RegisterOutcome
import io.paritytech.polkadotapp.feature_videogame_impl.domain.interactor.VideoGameChatBotFooterInteractor
import io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications.GameStartAlarmOffset
import io.paritytech.polkadotapp.feature_videogame_impl.presentation.bot.models.AlertSettingsUiState
import io.paritytech.polkadotapp.feature_videogame_impl.presentation.bot.models.FooterUiState
import io.paritytech.polkadotapp.feature_videogame_impl.presentation.bot.models.WeeklyGameDepositState
import io.paritytech.polkadotapp.feature_videogame_impl.presentation.bot.models.WeeklyGameFooterState
import io.paritytech.polkadotapp.feature_videogame_impl.presentation.bot.overlay.WeeklyGamePillVisibilityHolder
import io.paritytech.polkadotapp.feature_videogame_impl.presentation.common.models.toBotUi
import io.paritytech.polkadotapp.feature_videogame_impl.presentation.notifications.VideoGameNotificationsMixin
import io.paritytech.polkadotapp.feature_videogame_impl.utils.VideoGameLaunchCoordinator
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onStart
import kotlinx.coroutines.flow.runningReduce
import kotlinx.coroutines.flow.update
import timber.log.Timber
import javax.inject.Inject

@HiltViewModel
internal class WeeklyGameBotFooterViewModel @Inject constructor(
    private val interactor: VideoGameChatBotFooterInteractor,
    private val tokenAmountMapper: TokenAmountMapper,
    private val videoGameLaunchCoordinator: VideoGameLaunchCoordinator,
    private val router: VideoGameRouter,
    private val videoGameNotificationsMixin: VideoGameNotificationsMixin,
    private val calendarEventsMixin: CalendarEventsMixin,
    @ApplicationContext context: Context,
    gameInfoSyncService: VideoGameInfoSyncService,
    private val pillVisibility: WeeklyGamePillVisibilityHolder,
    dimSwitchMixinFactory: DimSwitchMixin.Factory
) : BaseViewModel(), WeeklyGameBotFooterContract {
    override val dimSwitchMixin = dimSwitchMixinFactory.create(
        currentDimId = Dim2CommitmentHandler.DIM_ID,
        onError = ::showError
    )

    private val registrationStage = interactor.subscribeRegistrationStage()
        .shareInBackground()

    private val registrationProgressFlow = MutableStateFlow(false)

    override val depositState = MutableStateFlow(WeeklyGameDepositState())

    private val calendarEventModel = gameInfoSyncService.subscribeCurrentActiveGameInfo()
        .map {
            it?.let {
                CalendarEvent(
                    title = context.gameCalendarEventTitle(),
                    timeStart = it.gameStartMillis,
                    duration = it.gameDuration()
                )
            }
        }
        .shareInBackground()

    private val isGameAddedToCalendar = calendarEventModel
        .flatMapLatest { event ->
            if (event == null) flowOf { true }
            else calendarEventsMixin.observeEventAddedToCalendar(event)
        }
        .shareInBackground()

    private val hideAddToCalendarButton = isGameAddedToCalendar
        .runningReduce { accumulator, newValue ->
            accumulator && newValue
        }
        .shareInBackground()

    private val calendarButtonState = isGameAddedToCalendar
        .combine(hideAddToCalendarButton) { added, hide -> added to hide }
        .shareInBackground()

    private val registrationState = combine(
        registrationStage,
        registrationProgressFlow,
        // Optimistic default: enabled until the airdrop gate says otherwise (non-airdrop games stay open).
        interactor.subscribeAirdropRegistrationReady().onStart { emit(true) },
    ) { stage, inProgress, airdropReady -> Triple(stage, inProgress, airdropReady) }
        .shareInBackground()

    private val isMember = interactor.subscribeIsMember()
        .onStart { emit(false) }
        .shareInBackground()

    override val uiState = combine(
        interactor.subscribeUpcomingGame(),
        registrationState,
        calendarButtonState,
        isMember,
        interactor.subscribeReadyToUpgradeUsername()
            .onStart { emit(UpgradeToFullUsernameState.NotReady) }
    ) { upcomingGameStart, registration, calendarButton, isMember, readyToUpgradeUsername ->
        val (registrationStage, registrationInProgress, isAirdropRegistrationReady) = registration
        val (isGameAddedToCalendar, hideAddToCalendarButton) = calendarButton
        FooterUiState(
            upcomingGameUiState = upcomingGameStart?.toBotUi(
                registrationStage = registrationStage,
                isRegistrationInProgress = registrationInProgress,
                isGameAddedToCalendar = isGameAddedToCalendar,
                hideAddToCalendarButton = hideAddToCalendarButton,
                isMember = isMember,
                isAirdropRegistrationReady = isAirdropRegistrationReady,
            ),
            upgradeUsernameUiState = readyToUpgradeUsername.toBotUi()
        )
    }
        .stateInBackground(SharingStarted.Eagerly, FooterUiState(null, null))

    override val footerState = interactor.subscribeFooterState()
        .stateInBackground(SharingStarted.Eagerly, WeeklyGameFooterState.Loading)

    override fun setUpcomingWidgetVisible(visible: Boolean) {
        pillVisibility.setFooterVisible(visible)
    }

    override fun setInlinePillVisible(visible: Boolean) {
        pillVisibility.setInlinePillVisible(visible)
    }

    override val alertSettingsState = MutableStateFlow(
        AlertSettingsUiState(selectedOffset = interactor.getAlarmOffset())
    )

    override fun register() = launchUnit {
        if (registrationProgressFlow.value) return@launchUnit
        withRegistrationInProgress {
            interactor.register()
                .onSuccess { outcome ->
                    when (outcome) {
                        RegisterOutcome.Submitted -> onRegistrationSuccess()
                        is RegisterOutcome.NeedsDeposit -> showDepositPrompt(outcome.requiredDeposit)
                    }
                }
                .onFailure { showError(it) }
        }
    }

    private fun showDepositPrompt(requiredDeposit: ChainAssetWithAmount) {
        depositState.update {
            it.copy(
                isVisible = true,
                requiredAmount = tokenAmountMapper.mapFrom(requiredDeposit)
            )
        }
    }

    private inline fun <R> withRegistrationInProgress(block: () -> R): R {
        registrationProgressFlow.enable()
        return try {
            block()
        } finally {
            registrationProgressFlow.disable()
        }
    }

    private suspend fun onRegistrationSuccess() {
        videoGameNotificationsMixin.checkPermissionsAndScheduleGameReminders()
        Timber.d("successfully registered")
    }

    override fun startGame() = launchUnit {
        videoGameLaunchCoordinator.launchGame()
    }

    override fun addToCalendar() = launchUnit {
        val event = calendarEventModel.filterNotNull().first()
        calendarEventsMixin.addEvent(event)
            .onFailure {
                Timber.e(it, "failed to add event to calendar")
                showError(it)
            }
    }

    override fun deposit() = launchUnit {
        depositState.update { it.copy(inProgress = true) }

        withRegistrationInProgress {
            interactor.deposit()
                .onSuccess {
                    depositState.update {
                        it.copy(
                            isVisible = false,
                            inProgress = false
                        )
                    }
                    onRegistrationSuccess()
                }
                .onFailure { t ->
                    depositState.update { it.copy(inProgress = false) }
                    Timber.e(t, "Deposit/registration failed")
                    showError(t)
                }
        }
    }

    override fun cancelDeposit() {
        depositState.update { it.copy(isVisible = false) }
    }

    override fun onUpgradeUsernameClick() {
        router.openUpgradeUsername()
    }

    override fun onAlertOffsetSelect(offset: GameStartAlarmOffset) = launchUnit {
        interactor.setAlarmOffset(offset)
        alertSettingsState.update { it.copy(selectedOffset = offset) }
        interactor.rescheduleGameStartAlarm()
    }
}
