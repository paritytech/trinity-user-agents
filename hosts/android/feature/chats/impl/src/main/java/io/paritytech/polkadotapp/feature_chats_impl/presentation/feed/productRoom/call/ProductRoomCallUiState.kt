package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.call

import androidx.compose.runtime.Immutable
import io.paritytech.polkadotapp.design.components.avatar.AvatarUiModel
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.ProductToolbarUiModel
import kotlinx.collections.immutable.persistentListOf
import kotlinx.collections.immutable.toImmutableList

/** What the room screen shows of the call of a product room (07-LATER §11.2). */
@Immutable
data class ProductRoomCallUiState(
    /** The call buttons of the header, or null when the room shows none. */
    val buttons: Buttons?,
    val call: Call?,
    /** The call screen of the host shows. Only a call that a session of the Media service carries opens it. */
    val showsCallScreen: Boolean,
) {
    @Immutable
    data class Buttons(val audio: Boolean, val video: Boolean)

    @Immutable
    sealed interface Call {
        val callId: String
        val video: Boolean

        /** A call to the user. The banner of the room shows the caller, Accept and Decline. */
        data class Ringing(
            override val callId: String,
            override val video: Boolean,
            val callerName: String?,
            val callerAvatar: AvatarUiModel?,
        ) : Call

        /** A call that continues since [startedAt] (milliseconds since the Unix epoch). */
        data class Active(
            override val callId: String,
            override val video: Boolean,
            val startedAt: Long,
            /** A session of the Media service carries the call, so a tap opens the call screen of the host. */
            val hasMedia: Boolean,
        ) : Call
    }

    companion object {
        val None = ProductRoomCallUiState(buttons = null, call = null, showsCallScreen = false)
    }
}

/** What the user does with the call of a product room. */
sealed interface ProductRoomCallAction {
    /** A press on a call button of the header. */
    data class Request(val video: Boolean) : ProductRoomCallAction

    /** Accept or Decline on the ringing banner. */
    data class Answer(val accept: Boolean) : ProductRoomCallAction

    /** A tap on the call bar. */
    data object Open : ProductRoomCallAction

    /** End on the call bar or on the call screen. */
    data object End : ProductRoomCallAction

    data object CloseScreen : ProductRoomCallAction

    /** The room screen shows or stops to show: the alerts outside the room stop for this room while it shows. */
    data class ScreenShown(val shown: Boolean) : ProductRoomCallAction
}

/** The time since the start of a call: `m:ss`, or `h:mm:ss` from one hour (as iOS). */
fun callElapsedText(startedAt: Long, now: Long): String {
    val seconds = ((now - startedAt) / MILLIS_IN_SECOND).coerceAtLeast(0)
    val hours = seconds / SECONDS_IN_HOUR
    val minutes = seconds % SECONDS_IN_HOUR / SECONDS_IN_MINUTE
    val rest = seconds % SECONDS_IN_MINUTE
    return if (hours > 0) "%d:%02d:%02d".format(hours, minutes, rest) else "%d:%02d".format(minutes, rest)
}

private const val MILLIS_IN_SECOND = 1_000L
private const val SECONDS_IN_MINUTE = 60L
private const val SECONDS_IN_HOUR = 3_600L

/** The toolbar with every action of the product in the overflow menu: the call buttons take the bar. */
internal fun ProductToolbarUiModel.allInOverflow(): ProductToolbarUiModel =
    if (inline.isEmpty()) this else ProductToolbarUiModel(inline = persistentListOf(), overflow = (inline + overflow).toImmutableList())
