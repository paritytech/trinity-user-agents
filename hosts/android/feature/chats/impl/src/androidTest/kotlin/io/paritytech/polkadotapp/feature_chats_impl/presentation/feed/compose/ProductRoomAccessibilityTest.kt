package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose

import androidx.compose.ui.semantics.AccessibilityAction
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.SemanticsPropertyKey
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.hasAnyDescendant
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.paritytech.polkadotapp.design.components.avatar.AvatarUiModel
import io.paritytech.polkadotapp.design.configs.colors.AvatarColorScheme
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.designsystem.themes.PolkadotAppTheme
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.ChatRoomUnavailableLine
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.ProductParticipantsButton
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.menu.ParticipantsListContent
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.components.MessageReactions
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.ChatParticipantUiModel
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.ParticipantListUiState
import kotlinx.collections.immutable.persistentListOf
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import io.paritytech.polkadotapp.common.R as RCommon

/** TalkBack reads each new control of a product room (P8.8). */
@RunWith(AndroidJUnit4::class)
class ProductRoomAccessibilityTest {
    @get:Rule
    val compose = createComposeRule()

    private val resources = InstrumentationRegistry.getInstrumentation().targetContext.resources

    @Test
    fun aReactionChipReadsItsEmojiCountAndStateWithLabelledActions() {
        compose.setContent {
            PolkadotTheme(theme = PolkadotAppTheme.DEFAULT) {
                MessageReactions(
                    reactions = persistentListOf(ChatMessageUiModel.Reaction(count = 3, emoji = "👍", reactedByUser = true)),
                    onReactionClick = {},
                    onReactionLongClick = {},
                )
            }
        }

        val description = resources.getQuantityString(RCommon.plurals.chat_reaction_count, 3, "👍", 3)
        compose.onNodeWithContentDescription(description)
            .assert(SemanticsMatcher.expectValue(SemanticsProperties.StateDescription, string(RCommon.string.chat_reaction_yours)))
            .assert(hasActionLabel(SemanticsActions.OnClick, string(RCommon.string.chat_reaction_toggle)))
            .assert(hasActionLabel(SemanticsActions.OnLongClick, string(RCommon.string.chat_reaction_show_who)))
    }

    @Test
    fun theParticipantsButtonHasALabel() {
        compose.setContent {
            PolkadotTheme(theme = PolkadotAppTheme.DEFAULT) {
                ProductParticipantsButton(onClick = {})
            }
        }

        compose.onNodeWithContentDescription(string(RCommon.string.chat_participants_title))
            .assert(SemanticsMatcher.keyIsDefined(SemanticsActions.OnClick))
    }

    @Test
    fun theUnavailableLineAnnouncesItsChanges() {
        compose.setContent {
            PolkadotTheme(theme = PolkadotAppTheme.DEFAULT) {
                ChatRoomUnavailableLine(isLoadFailure = true, onRetry = {})
            }
        }

        compose.onNode(
            SemanticsMatcher.expectValue(SemanticsProperties.LiveRegion, LiveRegionMode.Polite) and
                hasAnyDescendant(hasText(string(RCommon.string.chat_feed_page_failed))),
        ).assertExists()
    }

    @Test
    fun theParticipantListHasAHeadingAndRowsThatSayWhatATapDoes() {
        compose.setContent {
            PolkadotTheme(theme = PolkadotAppTheme.DEFAULT) {
                ParticipantsListContent(
                    state = ParticipantListUiState(
                        participants = persistentListOf(
                            ChatParticipantUiModel("alice", "Alice", "Guest", AvatarUiModel.Name("Alice", AvatarColorScheme.from("alice".encodeToByteArray()))),
                        ),
                        loading = false,
                        failed = false,
                        hasMore = false,
                    ),
                    onLoadMore = {},
                    onSelect = {},
                )
            }
        }

        compose.onNodeWithText(string(RCommon.string.chat_participants_title)).assert(SemanticsMatcher.keyIsDefined(SemanticsProperties.Heading))
        compose.onNode(hasText("Alice", substring = true) and hasActionLabel(SemanticsActions.OnClick, string(RCommon.string.chat_participant_show)))
            .assertExists()
    }

    private fun string(id: Int) = resources.getString(id)

    private fun hasRole(role: Role) = SemanticsMatcher.expectValue(SemanticsProperties.Role, role)

    private fun hasActionLabel(action: SemanticsPropertyKey<AccessibilityAction<() -> Boolean>>, label: String) =
        SemanticsMatcher("${action.name} labelled $label") { node -> node.config.getOrNull(action)?.label == label }
}
