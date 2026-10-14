package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.dialog.components

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.compose.ui.util.fastForEachIndexed
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.ImmutableSet
import kotlinx.collections.immutable.persistentSetOf
import kotlinx.collections.immutable.toImmutableList
import kotlinx.coroutines.launch

private val PICKER_HEIGHT = 280.dp
private const val EMOJIS_PER_ROW = 8
private val EMOJI_BUTTON_SIZE = 36.dp
private val CATEGORY_TAB_SIZE = 32.dp
private const val CONTENT_TYPE_HEADER = "header"
private const val CONTENT_TYPE_EMOJI_ROW = "emoji_row"

private val CHUNKED_CATEGORIES = EMOJI_CATEGORIES.map { it.emojis.chunked(EMOJIS_PER_ROW) }
private val CATEGORY_START_INDICES = CHUNKED_CATEGORIES.runningFold(0) { acc, rows -> acc + 1 + rows.size }.dropLast(1)

@Composable
internal fun ExpandedEmojiPicker(
    modifier: Modifier = Modifier,
    userReactedEmojis: ImmutableSet<String>,
    onEmojiClick: (String) -> Unit
) {
    val listState = rememberLazyListState()
    val coroutineScope = rememberCoroutineScope()

    val activeCategoryIndex by remember {
        derivedStateOf {
            if (!listState.canScrollForward) return@derivedStateOf CATEGORY_START_INDICES.lastIndex

            val insertion = CATEGORY_START_INDICES.binarySearch(listState.firstVisibleItemIndex)
            if (insertion >= 0) insertion else -(insertion + 1) - 1
        }
    }

    PolkadotSurface(
        modifier = modifier,
        shape = PolkadotTheme.shapes.large,
        color = PolkadotTheme.colors.bg.surface.container,
        border = BorderStroke(PolkadotTheme.borders.default, PolkadotTheme.colors.stroke.secondary)
    ) {
        Column {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(
                        horizontal = PolkadotTheme.spacings.small,
                        vertical = PolkadotTheme.spacings.tiny
                    ),
                horizontalArrangement = Arrangement.SpaceEvenly,
                verticalAlignment = Alignment.CenterVertically
            ) {
                EMOJI_CATEGORIES.fastForEachIndexed { index, category ->
                    CategoryTab(
                        icon = category.icon,
                        isActive = index == activeCategoryIndex,
                        onClick = {
                            coroutineScope.launch {
                                listState.scrollToItem(CATEGORY_START_INDICES[index])
                            }
                        }
                    )
                }
            }

            LazyColumn(
                state = listState,
                modifier = Modifier
                    .fillMaxWidth()
                    .height(PICKER_HEIGHT),
                verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.extraTiny)
            ) {
                EMOJI_CATEGORIES.fastForEachIndexed { catIndex, category ->
                    val headerKey = CATEGORY_START_INDICES[catIndex]

                    item(
                        key = headerKey,
                        contentType = CONTENT_TYPE_HEADER
                    ) {
                        NovaText(
                            modifier = Modifier.padding(
                                horizontal = PolkadotTheme.spacings.mediumIncreased,
                                vertical = PolkadotTheme.spacings.extraMedium
                            ),
                            text = stringResource(category.labelResId),
                            style = PolkadotTheme.typography.body.mediumEmphasized,
                            color = PolkadotTheme.colors.fg.tertiary
                        )
                    }

                    val rows = CHUNKED_CATEGORIES[catIndex]

                    items(
                        count = rows.size,
                        key = { rowIndex -> headerKey + 1 + rowIndex },
                        contentType = { CONTENT_TYPE_EMOJI_ROW }
                    ) { rowIndex ->
                        EmojiRow(
                            emojis = rows[rowIndex].toImmutableList(),
                            userReactedEmojis = userReactedEmojis,
                            onEmojiClick = onEmojiClick
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun EmojiRow(
    emojis: ImmutableList<String>,
    userReactedEmojis: ImmutableSet<String>,
    onEmojiClick: (String) -> Unit
) {
    val textMeasurer = rememberTextMeasurer()
    val textStyle = PolkadotTheme.typography.headline.small
    val selectedColor = PolkadotTheme.colors.bg.action.tertiary
    val buttonSizePx = with(LocalDensity.current) { EMOJI_BUTTON_SIZE.toPx() }

    val measurements = remember(emojis) {
        emojis.map { textMeasurer.measure(it, textStyle) }
    }

    Canvas(
        modifier = Modifier
            .fillMaxWidth()
            .height(EMOJI_BUTTON_SIZE)
            .pointerInput(emojis) {
                detectTapGestures { offset ->
                    val columnWidth = size.width / EMOJIS_PER_ROW
                    val index = (offset.x / columnWidth).toInt()
                    if (index in emojis.indices) {
                        onEmojiClick(emojis[index])
                    }
                }
            }
    ) {
        val columnWidth = size.width / EMOJIS_PER_ROW
        emojis.forEachIndexed { index, emoji ->
            val centerX = columnWidth * index + columnWidth / 2
            val centerY = size.height / 2

            if (emoji in userReactedEmojis) {
                drawCircle(
                    color = selectedColor,
                    radius = buttonSizePx / 2,
                    center = Offset(centerX, centerY)
                )
            }

            val measurement = measurements[index]
            drawText(
                textLayoutResult = measurement,
                topLeft = Offset(
                    centerX - measurement.size.width / 2f,
                    centerY - measurement.size.height / 2f
                )
            )
        }
    }
}

@Composable
private fun CategoryTab(
    icon: String,
    isActive: Boolean,
    onClick: () -> Unit
) {
    PolkadotSurface(
        shape = PolkadotTheme.shapes.full,
        color = if (isActive) PolkadotTheme.colors.bg.action.tertiary else PolkadotTheme.colors.bg.surface.container,
        onClick = onClick
    ) {
        Box(
            modifier = Modifier.size(CATEGORY_TAB_SIZE),
            contentAlignment = Alignment.Center
        ) {
            NovaText(
                text = icon,
                style = PolkadotTheme.typography.body.large
            )
        }
    }
}

@Preview
@Composable
private fun ExpandedEmojiPickerPreview() {
    PolkadotTheme {
        ExpandedEmojiPicker(
            userReactedEmojis = persistentSetOf("😀", "❤️"),
            onEmojiClick = {}
        )
    }
}
