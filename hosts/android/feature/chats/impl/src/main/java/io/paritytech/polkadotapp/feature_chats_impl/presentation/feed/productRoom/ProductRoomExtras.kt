package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom

import androidx.compose.runtime.Immutable
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatCommandModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatSuggestionModel
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.persistentListOf
import kotlinx.collections.immutable.toImmutableList

/** The quick replies, the slash commands and the activity line of a product room (02-DESIGN §6.6, P6.8). */
@Immutable
data class ProductRoomExtrasUiState(
    val suggestions: ImmutableList<ChatSuggestionModel>,
    val commands: ImmutableList<ChatCommandModel>,
    /** The activity line, for example "Bot is typing". Null shows no line. */
    val activity: String?,
    /** The persona that the activity line is about, drawn with its icon (07-LATER §5.2). */
    val activitySpeaker: ChatParticipantUiModel? = null,
) {
    companion object {
        val None = ProductRoomExtrasUiState(suggestions = persistentListOf(), commands = persistentListOf(), activity = null)
    }
}

/**
 * The commands that the catalogue offers for the text of the input: typing `/` opens the catalogue, and the name typed
 * after it filters the commands by prefix. The catalogue closes once the name has a space after it.
 */
fun ImmutableList<ChatCommandModel>.catalogueFor(input: String): ImmutableList<ChatCommandModel> {
    if (!input.startsWith(COMMAND_PREFIX) || input.any { it.isWhitespace() }) return persistentListOf()

    val typed = input.removePrefix(COMMAND_PREFIX)
    return filter { it.name.startsWith(typed, ignoreCase = true) }.toImmutableList()
}

/** The text that choosing [command] puts in the input. */
fun ChatCommandModel.inputText(): String = "$COMMAND_PREFIX$name "

private const val COMMAND_PREFIX = "/"
