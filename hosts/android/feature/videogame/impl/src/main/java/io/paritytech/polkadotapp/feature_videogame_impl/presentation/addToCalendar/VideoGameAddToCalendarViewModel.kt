package io.paritytech.polkadotapp.feature_videogame_impl.presentation.addToCalendar

import android.content.Context
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.presentation.sharing.SharingManager
import io.paritytech.polkadotapp.common.utils.EventCalendarSharing
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.feature_videogame_impl.VideoGameRouter
import io.paritytech.polkadotapp.feature_videogame_impl.data.VideoGameInfoSyncService
import io.paritytech.polkadotapp.feature_videogame_impl.data.calendar.gameCalendarEventTitle
import io.paritytech.polkadotapp.feature_videogame_impl.data.getCurrentActiveGameInfo
import javax.inject.Inject

class VideoGameAddToCalendarViewModel @Inject constructor(
    private val sharingManager: SharingManager,
    private val router: VideoGameRouter,
    private val gameInfoSyncService: VideoGameInfoSyncService,
    @ApplicationContext private val context: Context,
) : BaseViewModel(), VideoGameAddToCalendarContract {
    override fun confirm() = launchUnit {
        sharingManager.shareCalendarEvent(
            EventCalendarSharing(
                title = context.gameCalendarEventTitle(),
                startTime = gameInfoSyncService.getCurrentActiveGameInfo().gameStartMillis
            )
        )

        router.back()
    }

    override fun decline() {
        router.back()
    }
}
