package io.paritytech.polkadotapp.feature_chats_impl.presentation.search.compose

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.EnterTransition
import androidx.compose.animation.ExitTransition
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onInterceptKeyBeforeSoftKeyboard
import androidx.compose.ui.input.key.type
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.hilt.lifecycle.viewmodel.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.common.presentation.notification.rememberAppNotifier
import io.paritytech.polkadotapp.common.presentation.screens.ObserveViewModelEvents
import io.paritytech.polkadotapp.common.utils.randomBytes
import io.paritytech.polkadotapp.design.components.avatar.AvatarUiModel
import io.paritytech.polkadotapp.design.components.avatar.Mock
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonShape
import io.paritytech.polkadotapp.design.components.button.icon.PolkadotIconButton
import io.paritytech.polkadotapp.design.components.button.icon.PolkadotIconButtonSize
import io.paritytech.polkadotapp.design.components.docklet.DockletGeometry
import io.paritytech.polkadotapp.design.components.docklet.DockletGlassTheme
import io.paritytech.polkadotapp.design.components.docklet.DockletSearchButton
import io.paritytech.polkadotapp.design.components.docklet.DockletSearchField
import io.paritytech.polkadotapp.design.components.docklet.LocalDockletPalette
import io.paritytech.polkadotapp.design.components.docklet.dockletContentsTransition
import io.paritytech.polkadotapp.design.components.docklet.dockletFieldOutline
import io.paritytech.polkadotapp.design.components.docklet.rememberDockletFieldButtonStyle
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ScanFrame
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_impl.presentation.chatSearch.models.NoRowStatus
import io.paritytech.polkadotapp.feature_chats_impl.presentation.chatSearch.models.RecentChatUiModel
import io.paritytech.polkadotapp.feature_chats_impl.presentation.search.AddContactSearchResults
import io.paritytech.polkadotapp.feature_chats_impl.presentation.search.AddContactUiState
import io.paritytech.polkadotapp.feature_chats_impl.presentation.search.AddContactViewModel
import io.paritytech.polkadotapp.feature_chats_impl.presentation.search.compose.components.AddContactSearchContent
import io.paritytech.polkadotapp.feature_chats_impl.presentation.search.compose.components.AddContactSearchHeader
import io.paritytech.polkadotapp.feature_chats_impl.presentation.search.models.UserSearchResultUiModel
import io.paritytech.polkadotapp.feature_chats_impl.presentation.util.rememberCompositionViewModelStoreOwner
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.persistentListOf
import kotlinx.collections.immutable.toImmutableList
import kotlin.random.Random
import io.paritytech.polkadotapp.common.R as RCommon

private val ScanTileSize = 110.dp
private val ScanTileTop = 19.dp
private val ScanTileRadius = 24.dp
private val SearchButtonGap = 9.dp

enum class AddContactStep { Scan, Search }

@Composable
fun AddContactPanel(
    modifier: Modifier,
    step: AddContactStep,
    keyboardRequested: Boolean,
    onSearchFieldClick: () -> Unit,
    onScanClick: () -> Unit,
    onClose: () -> Unit,
    scanner: @Composable (modifier: Modifier) -> Unit,
) {
    val viewModel = hiltViewModel<AddContactViewModel>(viewModelStoreOwner = rememberCompositionViewModelStoreOwner())
    val state by viewModel.state.collectAsStateWithLifecycle()

    ObserveViewModelEvents(viewModel, rememberAppNotifier())

    val actions = remember(viewModel, onSearchFieldClick, onScanClick, onClose) {
        AddContactPanelActions(
            onSearchFieldClick = onSearchFieldClick,
            onScanClick = onScanClick,
            onClose = onClose,
            onSearchChange = viewModel::onSearchChange,
            onSearchResultClick = viewModel::onSearchResultClick,
            onRecentClick = viewModel::onRecentClick,
        )
    }

    AddContactPanelInternal(
        modifier = modifier,
        step = step,
        state = state,
        keyboardRequested = keyboardRequested,
        actions = actions,
        scanner = scanner,
    )
}

@Composable
private fun AddContactPanelInternal(
    modifier: Modifier,
    step: AddContactStep,
    state: AddContactUiState,
    keyboardRequested: Boolean,
    actions: AddContactPanelActions,
    scanner: @Composable (modifier: Modifier) -> Unit,
) {
    AnimatedContent(
        modifier = modifier,
        targetState = step,
        transitionSpec = { EnterTransition.None togetherWith ExitTransition.None },
        label = "AddContactStep",
    ) { shownStep ->
        val stepModifier = Modifier
            .fillMaxSize()
            .then(dockletContentsTransition())
        when (shownStep) {
            AddContactStep.Scan -> AddContactScanStep(
                modifier = stepModifier.padding(horizontal = PolkadotTheme.spacings.mediumIncreased),
                actions = actions,
                scanner = scanner,
            )

            AddContactStep.Search -> AddContactSearchStep(
                modifier = stepModifier.padding(
                    start = PolkadotTheme.spacings.mediumIncreased,
                    end = PolkadotTheme.spacings.mediumIncreased,
                    top = DockletGeometry.Step2ContentTop,
                    bottom = DockletGeometry.Step2ContentBottom,
                ),
                state = state,
                keyboardRequested = keyboardRequested,
                actions = actions,
            )
        }
    }
}

@Composable
private fun AddContactScanStep(
    modifier: Modifier,
    actions: AddContactPanelActions,
    scanner: @Composable (modifier: Modifier) -> Unit,
) {
    Column(
        modifier = modifier,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        VerticalSpacer { ScanTileTop }
        PolkadotSurface(
            modifier = Modifier.size(ScanTileSize),
            shape = RoundedCornerShape(ScanTileRadius),
            color = PolkadotTheme.colors.bg.surface.container,
        ) {
            scanner(Modifier.fillMaxSize())
        }
        VerticalSpacer { SearchButtonGap }
        DockletSearchButton(
            modifier = Modifier.fillMaxWidth(),
            text = stringResource(RCommon.string.common_search),
            onClick = actions.onSearchFieldClick,
        )
    }
}

@Composable
private fun AddContactSearchStep(
    modifier: Modifier,
    state: AddContactUiState,
    keyboardRequested: Boolean,
    actions: AddContactPanelActions,
) {
    val focusRequester = remember { FocusRequester() }
    LaunchedEffect(keyboardRequested) {
        if (keyboardRequested) focusRequester.requestFocus()
    }

    Column(modifier = modifier) {
        AddContactSearchHeader(query = state.searchQuery)
        VerticalSpacer { tiny }
        Box(modifier = Modifier.weight(1f)) {
            AddContactSearchContent(
                state = state,
                onSearchResultClick = actions.onSearchResultClick,
                onRecentClick = actions.onRecentClick,
            )
        }
        VerticalSpacer { tiny }
        Row(verticalAlignment = Alignment.CenterVertically) {
            val shape = PolkadotButtonShape.pill
            PolkadotIconButton(
                modifier = Modifier.dockletFieldOutline(shape, LocalDockletPalette.current),
                icon = NovaIcons.ScanFrame,
                onClick = actions.onScanClick,
                style = rememberDockletFieldButtonStyle(),
                size = PolkadotIconButtonSize.mediumIncreased(),
                shape = shape,
            )
            HorizontalSpacer { smallIncreased }
            DockletSearchField(
                modifier = Modifier
                    .weight(1f)
                    .focusRequester(focusRequester)
                    .onInterceptKeyBeforeSoftKeyboard { event ->
                        val back = event.key == Key.Back
                        if (back && event.type == KeyEventType.KeyUp) actions.onClose()
                        back
                    },
                value = state.searchQuery,
                onValueChange = actions.onSearchChange,
                placeholder = stringResource(RCommon.string.add_contact_search_placeholder),
            )
        }
    }
}

@Immutable
private data class AddContactPanelActions(
    val onSearchFieldClick: () -> Unit,
    val onScanClick: () -> Unit,
    val onClose: () -> Unit,
    val onSearchChange: (String) -> Unit,
    val onSearchResultClick: (UserSearchResultUiModel) -> Unit,
    val onRecentClick: (ChatId) -> Unit,
)

@Composable
private fun AddContactPanelPreview(state: AddContactUiState, step: AddContactStep) {
    PolkadotTheme {
        DockletGlassTheme {
            PolkadotSurface(color = LocalDockletPalette.current.glassFill) {
                AddContactPanelInternal(
                    modifier = Modifier
                        .fillMaxWidth()
                        .size(width = 382.dp, height = if (step == AddContactStep.Scan) 204.dp else 424.dp),
                    step = step,
                    state = state,
                    keyboardRequested = false,
                    actions = AddContactPanelActions(
                        onSearchFieldClick = {},
                        onScanClick = {},
                        onClose = {},
                        onSearchChange = {},
                        onSearchResultClick = {},
                        onRecentClick = {},
                    ),
                    scanner = { modifier -> Box(modifier = modifier) },
                )
            }
        }
    }
}

@Preview(widthDp = 382)
@Composable
private fun AddContactPanelScannerPreview() {
    AddContactPanelPreview(
        state = previewState(query = "", results = AddContactSearchResults.Idle),
        step = AddContactStep.Scan,
    )
}

@Preview(widthDp = 382)
@Composable
private fun AddContactPanelRecentsPreview() {
    AddContactPanelPreview(
        state = previewState(
            query = "",
            results = AddContactSearchResults.Idle,
            recents = previewRecents("mosticRiver.88", "delaware.01", "franz", "dmitry.01", "euclid.01"),
        ),
        step = AddContactStep.Search,
    )
}

@Preview(widthDp = 382)
@Composable
private fun AddContactPanelResultsPreview() {
    AddContactPanelPreview(
        state = previewState(
            query = "Mo",
            results = AddContactSearchResults.Sections(
                recents = previewRecents("mosticRiver.88"),
                allUsers = previewUsers("monster.01", "molecule", "mostwanted", "morales"),
            ),
        ),
        step = AddContactStep.Search,
    )
}

private fun previewRecents(vararg names: String) = names.map { name ->
    RecentChatUiModel(
        chatId = ChatId.fromContact(Random.randomBytes(32).intoAccountId()),
        key = name,
        title = name,
        avatarModel = AvatarUiModel.Mock.fromName(name),
        status = NoRowStatus,
        isMenuOpen = false,
    )
}.toImmutableList()

private fun previewUsers(vararg names: String) = names.map { name ->
    UserSearchResultUiModel(
        contactAccountId = Random.randomBytes(32).intoAccountId(),
        username = name,
        avatarModel = AvatarUiModel.Mock.fromName(name),
    )
}.toImmutableList()

private fun previewState(
    query: String,
    results: AddContactSearchResults,
    recents: ImmutableList<RecentChatUiModel> = persistentListOf(),
) = AddContactUiState(
    searchQuery = query,
    results = results,
    loadingContactId = null,
    recents = recents,
)
