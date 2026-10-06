package io.paritytech.polkadotapp.feature_videogame_impl.di

import androidx.hilt.lifecycle.viewmodel.compose.hiltViewModel
import dagger.Binds
import dagger.Lazy
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import dagger.multibindings.ElementsIntoSet
import dagger.multibindings.IntoMap
import dagger.multibindings.IntoSet
import dagger.multibindings.StringKey
import io.paritytech.polkadotapp.chains.storage.source.query.intercept.StorageQueryInterceptor
import io.paritytech.polkadotapp.common.data.network.NetworkApiCreator
import io.paritytech.polkadotapp.common.data.network.OverrideBaseUrlInterceptor
import io.paritytech.polkadotapp.common.data.network.TestnetEnvironment
import io.paritytech.polkadotapp.common.data.network.addDebugLoggingInterceptor
import io.paritytech.polkadotapp.common.presentation.AppInitializer
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.feature_account_api.domain.model.AliasAccountDerivationOverride
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.ChatExtension
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatOverlay
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatHeaderRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatOriginCustomConfiguration
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_api.domain.getTldRetrying
import io.paritytech.polkadotapp.feature_people_api.data.AliasContextProvider
import io.paritytech.polkadotapp.feature_people_api.data.SetAliasContext
import io.paritytech.polkadotapp.feature_people_api.domain.dim.DimCommitmentHandler
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.aliasAccountDerivationPath
import io.paritytech.polkadotapp.feature_products_api.domain.game.ProductGameReminder
import io.paritytech.polkadotapp.feature_products_api.presentation.SpaBrowserFragmentClass
import io.paritytech.polkadotapp.feature_videogame_api.data.repositories.VideoGameRepository
import io.paritytech.polkadotapp.feature_videogame_api.data.voucher.ScoreVouchersSyncManager
import io.paritytech.polkadotapp.feature_videogame_api.domain.collectibles.CollectiblesUrlResolver
import io.paritytech.polkadotapp.feature_videogame_api.domain.state.VideoGameKeepPlayingWarningUseCase
import io.paritytech.polkadotapp.feature_videogame_api.domain.state.VideoGamesProgressUseCase
import io.paritytech.polkadotapp.feature_videogame_api.domain.usecase.UpcomingGameStartUseCase
import io.paritytech.polkadotapp.feature_videogame_impl.VideoGameNotificationPublisher
import io.paritytech.polkadotapp.feature_videogame_impl.data.RealVideoGameInfoSyncService
import io.paritytech.polkadotapp.feature_videogame_impl.data.ScoreContextProvider
import io.paritytech.polkadotapp.feature_videogame_impl.data.VideoGameInfoSyncService
import io.paritytech.polkadotapp.feature_videogame_impl.data.calendar.RealProductGameCalendar
import io.paritytech.polkadotapp.feature_videogame_impl.data.collectibles.CollectiblesRepository
import io.paritytech.polkadotapp.feature_videogame_impl.data.collectibles.RealCollectiblesRepository
import io.paritytech.polkadotapp.feature_videogame_impl.data.collectibles.RealCollectiblesUrlResolver
import io.paritytech.polkadotapp.feature_videogame_impl.data.gameResults.AirdropRepository
import io.paritytech.polkadotapp.feature_videogame_impl.data.gameResults.GameResultsUrlProvider
import io.paritytech.polkadotapp.feature_videogame_impl.data.gameResults.RealAirdropRepository
import io.paritytech.polkadotapp.feature_videogame_impl.data.gameResults.RealGameResultsUrlProvider
import io.paritytech.polkadotapp.feature_videogame_impl.data.gameResults.VideoGameResultsPreloadInitializer
import io.paritytech.polkadotapp.feature_videogame_impl.data.notifications.RealVideoGameNotificationPublisher
import io.paritytech.polkadotapp.feature_videogame_impl.data.origins.RealScoreOrigins
import io.paritytech.polkadotapp.feature_videogame_impl.data.origins.ScoreOrigins
import io.paritytech.polkadotapp.feature_videogame_impl.data.repositories.DepositTrackingRepository
import io.paritytech.polkadotapp.feature_videogame_impl.data.repositories.GamePlayersRepository
import io.paritytech.polkadotapp.feature_videogame_impl.data.repositories.RealDepositTrackingRepository
import io.paritytech.polkadotapp.feature_videogame_impl.data.repositories.RealGamePlayersRepository
import io.paritytech.polkadotapp.feature_videogame_impl.data.repositories.RealScoreRepository
import io.paritytech.polkadotapp.feature_videogame_impl.data.repositories.RealVideoGameHistoryRepository
import io.paritytech.polkadotapp.feature_videogame_impl.data.repositories.RealVideoGameKeepPlayingWarningRepository
import io.paritytech.polkadotapp.feature_videogame_impl.data.repositories.RealVideoGameRepository
import io.paritytech.polkadotapp.feature_videogame_impl.data.repositories.RealVideoGameTooltipsRepository
import io.paritytech.polkadotapp.feature_videogame_impl.data.repositories.ScoreRepository
import io.paritytech.polkadotapp.feature_videogame_impl.data.repositories.VideoGameHistoryRepository
import io.paritytech.polkadotapp.feature_videogame_impl.data.repositories.VideoGameKeepPlayingWarningRepository
import io.paritytech.polkadotapp.feature_videogame_impl.data.repositories.VideoGameRepositoryInternal
import io.paritytech.polkadotapp.feature_videogame_impl.data.repositories.VideoGameTooltipsRepository
import io.paritytech.polkadotapp.feature_videogame_impl.data.scorePersonhoodContext
import io.paritytech.polkadotapp.feature_videogame_impl.data.storages.RealVideoGameHistoryRestoringStorage
import io.paritytech.polkadotapp.feature_videogame_impl.data.storages.VideoGameHistoryRestoringStorage
import io.paritytech.polkadotapp.feature_videogame_impl.data.telemetry.GameDashboardApi
import io.paritytech.polkadotapp.feature_videogame_impl.data.telemetry.GameDashboardTelemetryRepository
import io.paritytech.polkadotapp.feature_videogame_impl.data.telemetry.NoOpGameDashboardTelemetryRepository
import io.paritytech.polkadotapp.feature_videogame_impl.data.telemetry.RealGameDashboardTelemetryRepository
import io.paritytech.polkadotapp.feature_videogame_impl.data.tracked.LocalTxOverrideInterceptor
import io.paritytech.polkadotapp.feature_videogame_impl.data.voucher.RealScoreVouchersSyncManager
import io.paritytech.polkadotapp.feature_videogame_impl.domain.autoLaunch.VideoGameAutoLauncher
import io.paritytech.polkadotapp.feature_videogame_impl.domain.bot.WeeklyGameBot
import io.paritytech.polkadotapp.feature_videogame_impl.domain.chat.GameChatOriginConfiguration
import io.paritytech.polkadotapp.feature_videogame_impl.domain.collectibles.CollectiblesInteractor
import io.paritytech.polkadotapp.feature_videogame_impl.domain.collectibles.RealCollectiblesInteractor
import io.paritytech.polkadotapp.feature_videogame_impl.domain.dim.Dim2CommitmentHandler
import io.paritytech.polkadotapp.feature_videogame_impl.domain.gameResults.GameResultsInteractor
import io.paritytech.polkadotapp.feature_videogame_impl.domain.gameResults.RealGameResultsInteractor
import io.paritytech.polkadotapp.feature_videogame_impl.domain.interactor.ChatWithPlayersInteractor
import io.paritytech.polkadotapp.feature_videogame_impl.domain.interactor.RealChatWithPlayersInteractor
import io.paritytech.polkadotapp.feature_videogame_impl.domain.interactor.RealVideoGameChatBotFooterInteractor
import io.paritytech.polkadotapp.feature_videogame_impl.domain.interactor.RealVideoGameChatBotInteractor
import io.paritytech.polkadotapp.feature_videogame_impl.domain.interactor.RealVideoGameVoteInteractor
import io.paritytech.polkadotapp.feature_videogame_impl.domain.interactor.VideoGameChatBotFooterInteractor
import io.paritytech.polkadotapp.feature_videogame_impl.domain.interactor.VideoGameChatBotInteractor
import io.paritytech.polkadotapp.feature_videogame_impl.domain.interactor.VideoGameVoteInteractor
import io.paritytech.polkadotapp.feature_videogame_impl.domain.models.GameContactOrigins
import io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications.ProductGameCalendar
import io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications.ProductGameNotificationAutoCanceller
import io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications.ProductGameOsAccess
import io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications.RealProductGameReminder
import io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications.RealVideoGameReminderScheduler
import io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications.VideoGameNotificationAutoCanceller
import io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications.VideoGameReminderScheduler
import io.paritytech.polkadotapp.feature_videogame_impl.domain.snapshot.VideoGameSnapshotComputer
import io.paritytech.polkadotapp.feature_videogame_impl.domain.timeline.RealVideoGameTimelineService
import io.paritytech.polkadotapp.feature_videogame_impl.domain.timeline.VideoGameTimelineService
import io.paritytech.polkadotapp.feature_videogame_impl.domain.usecase.GameInvitationUseCase
import io.paritytech.polkadotapp.feature_videogame_impl.domain.usecase.PlayingAccountUseCase
import io.paritytech.polkadotapp.feature_videogame_impl.domain.usecase.RealGameInvitationUseCase
import io.paritytech.polkadotapp.feature_videogame_impl.domain.usecase.RealPlayingAccountUseCase
import io.paritytech.polkadotapp.feature_videogame_impl.domain.usecase.RealScheduleGameRemindersUseCase
import io.paritytech.polkadotapp.feature_videogame_impl.domain.usecase.RealUpcomingGameStartUseCase
import io.paritytech.polkadotapp.feature_videogame_impl.domain.usecase.RealVideoGameClaimCitizenshipUseCase
import io.paritytech.polkadotapp.feature_videogame_impl.domain.usecase.RealVideoGameJourneyUseCase
import io.paritytech.polkadotapp.feature_videogame_impl.domain.usecase.RealVideoGameKeepPlayingWarningUseCase
import io.paritytech.polkadotapp.feature_videogame_impl.domain.usecase.RealVideoGameOffboardingOptionUseCase
import io.paritytech.polkadotapp.feature_videogame_impl.domain.usecase.RealVideoGameRegistrationStageUseCase
import io.paritytech.polkadotapp.feature_videogame_impl.domain.usecase.RealVideoGameReportSubmittedUseCase
import io.paritytech.polkadotapp.feature_videogame_impl.domain.usecase.RealVideoGamesProgressUseCase
import io.paritytech.polkadotapp.feature_videogame_impl.domain.usecase.ScheduleGameRemindersUseCase
import io.paritytech.polkadotapp.feature_videogame_impl.domain.usecase.VideoGameClaimCitizenshipUseCase
import io.paritytech.polkadotapp.feature_videogame_impl.domain.usecase.VideoGameJourneyUseCase
import io.paritytech.polkadotapp.feature_videogame_impl.domain.usecase.VideoGameOffboardingOptionUseCase
import io.paritytech.polkadotapp.feature_videogame_impl.domain.usecase.VideoGameRegistrationStageUseCase
import io.paritytech.polkadotapp.feature_videogame_impl.domain.usecase.VideoGameReportSubmittedUseCase
import io.paritytech.polkadotapp.feature_videogame_impl.presentation.autoLaunch.ProductGameAutoOpener
import io.paritytech.polkadotapp.feature_videogame_impl.presentation.bot.overlay.GamePillOverlayRenderer
import io.paritytech.polkadotapp.feature_videogame_impl.presentation.bot.overlay.ProductGamePillOverlayViewModel
import io.paritytech.polkadotapp.feature_videogame_impl.presentation.bot.overlay.RealWeeklyGamePillVisibilityHolder
import io.paritytech.polkadotapp.feature_videogame_impl.presentation.bot.overlay.WeeklyGamePillVisibilityHolder
import io.paritytech.polkadotapp.feature_videogame_impl.presentation.notifications.RealProductGameOsAccess
import io.paritytech.polkadotapp.feature_videogame_impl.presentation.renderer.GameChatHeaderRenderer
import io.paritytech.polkadotapp.feature_videogame_impl.service.GestureAcceptanceChannel
import io.paritytech.polkadotapp.feature_videogame_impl.service.RealVideoGameStateHolder
import io.paritytech.polkadotapp.feature_videogame_impl.service.VideoGameSnapshotWriter
import io.paritytech.polkadotapp.feature_videogame_impl.service.VideoGameStateHolder
import io.paritytech.polkadotapp.feature_videogame_impl.service.VideoGameStateReader
import io.paritytech.polkadotapp.tools_remoteconfig_api.RemoteConfigService
import okhttp3.OkHttpClient
import javax.inject.Singleton

@InstallIn(SingletonComponent::class)
@Module
internal interface VideoGameFeatureModule {
    @Binds
    @IntoSet
    fun bindLocalTxOverrideInterceptor(impl: LocalTxOverrideInterceptor): StorageQueryInterceptor

    @Binds
    fun bindGamesProgressUseCase(impl: RealVideoGamesProgressUseCase): VideoGamesProgressUseCase

    @Binds
    fun bindVideoGameReportSubmittedUseCase(impl: RealVideoGameReportSubmittedUseCase): VideoGameReportSubmittedUseCase

    @Binds
    fun bindUpcomingGameStartUseCase(impl: RealUpcomingGameStartUseCase): UpcomingGameStartUseCase

    @Binds
    fun bindVideoGameRegistrationStageUseCase(impl: RealVideoGameRegistrationStageUseCase): VideoGameRegistrationStageUseCase

    @Binds
    fun bindVideoGameOffboardingOptionUseCase(impl: RealVideoGameOffboardingOptionUseCase): VideoGameOffboardingOptionUseCase

    @Binds
    fun bindGameOrigins(impl: RealScoreOrigins): ScoreOrigins

    @Binds
    fun bindVideoGameRepositoryInternal(impl: RealVideoGameRepository): VideoGameRepositoryInternal

    @Binds
    fun bindVideoGameRepository(impl: RealVideoGameRepository): VideoGameRepository

    @Binds
    fun bindVideoGameTooltipsRepository(impl: RealVideoGameTooltipsRepository): VideoGameTooltipsRepository

    @Binds
    fun bindVideoGameHistoryRepository(impl: RealVideoGameHistoryRepository): VideoGameHistoryRepository

    @Binds
    fun bindCollectiblesRepository(impl: RealCollectiblesRepository): CollectiblesRepository

    @Binds
    fun bindCollectiblesInteractor(impl: RealCollectiblesInteractor): CollectiblesInteractor

    @Binds
    fun bindCollectiblesUrlResolver(impl: RealCollectiblesUrlResolver): CollectiblesUrlResolver

    @Binds
    fun bindGamePlayersRepository(impl: RealGamePlayersRepository): GamePlayersRepository

    @Binds
    fun bindGameInfoSyncService(impl: RealVideoGameInfoSyncService): VideoGameInfoSyncService

    @Binds
    fun bindScoreRepository(impl: RealScoreRepository): ScoreRepository

    @Binds
    fun bindVideoGameReminderScheduler(impl: RealVideoGameReminderScheduler): VideoGameReminderScheduler

    @Binds
    fun bindProductGameReminder(impl: RealProductGameReminder): ProductGameReminder

    @Binds
    fun bindProductGameCalendar(impl: RealProductGameCalendar): ProductGameCalendar

    @Binds
    fun bindProductGameOsAccess(impl: RealProductGameOsAccess): ProductGameOsAccess

    @Binds
    fun bindVideoGameKeepPlayingWarningUseCase(impl: RealVideoGameKeepPlayingWarningUseCase): VideoGameKeepPlayingWarningUseCase

    @Binds
    fun bindVideoGameKeepPlayingWarningRepository(impl: RealVideoGameKeepPlayingWarningRepository): VideoGameKeepPlayingWarningRepository

    @Binds
    fun bindPlayingAccountUseCase(impl: RealPlayingAccountUseCase): PlayingAccountUseCase

    @Binds
    fun bindDepositTrackingRepository(impl: RealDepositTrackingRepository): DepositTrackingRepository

    @Binds
    fun bindResterScoreVoucherSyncManager(impl: RealScoreVouchersSyncManager): ScoreVouchersSyncManager

    @Binds
    fun bindGameInvitationUseCase(impl: RealGameInvitationUseCase): GameInvitationUseCase

    @Binds
    fun bindScheduleGameRemindersUseCase(impl: RealScheduleGameRemindersUseCase): ScheduleGameRemindersUseCase

    @Binds
    fun bindVideoGameClaimCitizenshipInteractor(impl: RealVideoGameClaimCitizenshipUseCase): VideoGameClaimCitizenshipUseCase

    @Binds
    fun bindVideoGameVoteInteractor(impl: RealVideoGameVoteInteractor): VideoGameVoteInteractor

    @Binds
    fun bindGameResultsInteractor(impl: RealGameResultsInteractor): GameResultsInteractor

    @Binds
    fun bindGameResultsUrlProvider(impl: RealGameResultsUrlProvider): GameResultsUrlProvider

    @Binds
    fun bindAirdropRepository(impl: RealAirdropRepository): AirdropRepository

    @Binds
    fun bindsVideoGameHistoryRestoringStorage(impl: RealVideoGameHistoryRestoringStorage): VideoGameHistoryRestoringStorage

    @Binds
    @Singleton
    @IntoSet
    fun bindWeeklyGameBot(impl: WeeklyGameBot): ChatExtension

    @Binds
    @Singleton
    fun bindVideoGameChatBotInteractor(impl: RealVideoGameChatBotInteractor): VideoGameChatBotInteractor

    @Binds
    fun bindVideoGameChatBotFooterInteractor(impl: RealVideoGameChatBotFooterInteractor): VideoGameChatBotFooterInteractor

    @Binds
    fun bindChatWithPlayersInteractor(impl: RealChatWithPlayersInteractor): ChatWithPlayersInteractor

    @Binds
    fun bindVideoGameNotificationPublisher(impl: RealVideoGameNotificationPublisher): VideoGameNotificationPublisher

    @Binds
    @IntoSet
    fun bindVideoGameNotificationAutoCanceller(impl: VideoGameNotificationAutoCanceller): AppInitializer

    @Binds
    @IntoSet
    fun bindVideoGameSnapshotComputer(impl: VideoGameSnapshotComputer): AppInitializer

    @Binds
    @IntoSet
    fun bindVideoGameAutoLauncher(impl: VideoGameAutoLauncher): AppInitializer

    @Binds
    @IntoSet
    fun bindProductGameAutoOpener(impl: ProductGameAutoOpener): AppInitializer

    @Binds
    @IntoSet
    fun bindProductGameNotificationAutoCanceller(impl: ProductGameNotificationAutoCanceller): AppInitializer

    @Binds
    @IntoSet
    fun bindVideoGameResultsPreloadInitializer(impl: VideoGameResultsPreloadInitializer): AppInitializer

    @Binds
    @IntoSet
    fun bindDim2CommitmentHandler(impl: Dim2CommitmentHandler): DimCommitmentHandler

    @Binds
    fun bindVideoGameJourneyUseCase(impl: RealVideoGameJourneyUseCase): VideoGameJourneyUseCase

    @Binds
    fun bindVideoGameStateHolder(impl: RealVideoGameStateHolder): VideoGameStateHolder

    @Binds
    fun bindVideoGameStateReader(impl: RealVideoGameStateHolder): VideoGameStateReader

    @Binds
    fun bindVideoGameSnapshotWriter(impl: RealVideoGameStateHolder): VideoGameSnapshotWriter

    @Binds
    fun bindGestureAcceptanceChannel(impl: RealVideoGameStateHolder): GestureAcceptanceChannel

    @Binds
    fun bindWeeklyGamePillVisibilityHolder(impl: RealWeeklyGamePillVisibilityHolder): WeeklyGamePillVisibilityHolder

    @Binds
    fun bindVideoGameTimelineService(impl: RealVideoGameTimelineService): VideoGameTimelineService

    companion object {
        @Provides
        @IntoMap
        @StringKey(GameContactOrigins.SHARED_GAME)
        fun provideGameChatHeaderRenderer(): CustomChatHeaderRenderer = GameChatHeaderRenderer()

        @Provides
        @IntoMap
        @StringKey(GameContactOrigins.SHARED_GAME)
        fun provideGameChatOriginConfiguration(): ChatOriginCustomConfiguration = GameChatOriginConfiguration()

        // Owned fragments hide an overlay: the pill stays off while a product is on screen.
        @Provides
        @IntoSet
        fun provideProductGamePill(@SpaBrowserFragmentClass spaBrowserFragmentClass: String): ChatOverlay = ChatOverlay(
            renderer = GamePillOverlayRenderer { hiltViewModel<ProductGamePillOverlayViewModel>() },
            ownedFragmentClasses = setOf(spaBrowserFragmentClass),
        )

        @Provides
        @ElementsIntoSet
        @SetAliasContext
        fun provideScoreContext(scoreContextProvider: ScoreContextProvider): Set<AliasContextProvider> {
            return setOf(AliasContextProvider { scoreContextProvider.context() })
        }

        // RFC-0024 resolves RFC-0022's deferral: the score context is owned by the personhood
        // product, so its alias account follows the owner's subtree rather than the game's. Existing
        // installs are not migrated - their alias account stays at the old dim2.dot path.
        @Provides
        @IntoSet
        fun provideScoreDerivationOverride(
            scoreContextProvider: ScoreContextProvider,
            dotNsTldProvider: DotNsTldProvider,
        ): AliasAccountDerivationOverride {
            return AliasAccountDerivationOverride(
                context = { scoreContextProvider.context() },
                derivationPath = {
                    scorePersonhoodContext(dotNsTldProvider.getTldRetrying()).aliasAccountDerivationPath()
                }
            )
        }

        // Own client so the game-dashboard base URL is rewritten per-request from remote config,
        // independent of the shared (identity-backend) client.
        @Provides
        @Singleton
        @GameDashboard
        fun provideGameDashboardOkHttpClient(
            builder: OkHttpClient.Builder,
            remoteConfigService: RemoteConfigService,
        ): OkHttpClient = builder
            .addInterceptor(
                OverrideBaseUrlInterceptor(GAME_DASHBOARD_SENTINEL_HOST) {
                    remoteConfigService.getSyncedString(GAME_DASHBOARD_URL_KEY).getOrThrow()
                }
            )
            .addDebugLoggingInterceptor()
            .build()

        // Telemetry is sent only on non-production environments. On production we bind a no-op so
        // callers need no environment checks and the dashboard client is never built (Lazy).
        @Provides
        @Singleton
        fun provideGameDashboardTelemetryRepository(
            environment: TestnetEnvironment,
            networkApiCreator: NetworkApiCreator,
            @GameDashboard okHttpClient: Lazy<OkHttpClient>,
            dispatchers: CoroutineDispatchers,
        ): GameDashboardTelemetryRepository = when (environment) {
            TestnetEnvironment.PRODUCTION -> NoOpGameDashboardTelemetryRepository

            TestnetEnvironment.TESTNET,
            TestnetEnvironment.NIGHTLY -> {
                val api = networkApiCreator.createRetrofit(
                    baseUrl = GAME_DASHBOARD_SENTINEL_URL,
                    customOkHttpClient = okHttpClient.get(),
                ).create(GameDashboardApi::class.java)

                RealGameDashboardTelemetryRepository(api, dispatchers)
            }
        }
    }
}
