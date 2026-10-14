package io.paritytech.polkadotapp.design.components.navigationbar

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.layout.Placeable
import androidx.compose.ui.layout.SubcomposeLayout
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.dp
import androidx.compose.ui.util.lerp
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ChatFilled
import io.paritytech.polkadotapp.design.components.icon.vectors.MoneyFilled
import io.paritytech.polkadotapp.design.components.icon.vectors.Search
import io.paritytech.polkadotapp.design.components.icon.vectors.Settings
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import kotlin.math.floor
import kotlin.math.roundToInt

private val IconSize = 28.dp
private val NotificationDotSize = 8.dp

// Breathing room when the bar hugs its content: packed at their measured width the items sit too tight, so
// the bar is stretched by this factor and the slack is shared out over the item slots.
private const val HUG_WIDTH_SCALE = 1.25f

private enum class NavBarSlot { Center, Items, Indicator }

/**
 * @param fillWidth spread the items across the whole available width. Off, the bar hugs its content: every
 * item gets the width of the widest one and the bar is only as wide as the items plus [centerContent].
 */
@Composable
fun PolkadotNavigationBar(
    selectedIndex: Int,
    itemCount: Int,
    modifier: Modifier = Modifier,
    shape: Shape = PolkadotTheme.shapes.full,
    color: Color = PolkadotTheme.colors.bg.surface.container,
    border: BorderStroke? = BorderStroke(PolkadotTheme.borders.default, PolkadotTheme.colors.stroke.primary),
    fillWidth: Boolean = true,
    centerContent: (@Composable () -> Unit)? = null,
    content: @Composable () -> Unit
) {
    val animatedIndex = animateFloatAsState(
        targetValue = selectedIndex.toFloat(),
        animationSpec = spring(
            dampingRatio = Spring.DampingRatioLowBouncy,
            stiffness = 700f
        ),
        label = "PolkadotNavigationBarSelectedIndex"
    )

    PolkadotSurface(
        modifier = modifier,
        shape = shape,
        color = color,
        border = border,
    ) {
        val indicatorColor = PolkadotTheme.colors.bg.surface.nested
        val indicatorShape = PolkadotTheme.shapes.full
        val overshoot = PolkadotTheme.spacings.tiny

        SubcomposeLayout(
            modifier = Modifier.padding(
                horizontal = PolkadotTheme.spacings.small,
                vertical = PolkadotTheme.spacings.tiny
            )
        ) { constraints ->
            val overshootPx = overshoot.roundToPx()

            val centerPlaceable = centerContent?.let {
                subcompose(NavBarSlot.Center, it).first()
                    .measure(constraints.copy(minWidth = 0, minHeight = 0))
            }
            val centerWidth = centerPlaceable?.width ?: 0

            val leftCount = if (centerPlaceable != null) (itemCount + 1) / 2 else itemCount
            val rightCount = itemCount - leftCount

            val itemMeasurables = subcompose(NavBarSlot.Items, content)

            val width: Int
            val slotWidth: Float
            val centerX: Int
            val rightStart: Float
            val itemPlaceables: List<Placeable>

            if (fillWidth) {
                width = constraints.maxWidth
                val halfWidth = (width - centerWidth) / 2
                slotWidth = if (centerPlaceable != null) {
                    halfWidth.toFloat() / maxOf(leftCount, rightCount).coerceAtLeast(1)
                } else {
                    width.toFloat() / itemCount.coerceAtLeast(1)
                }
                centerX = halfWidth
                rightStart = halfWidth + centerWidth + (halfWidth - rightCount * slotWidth) / 2f

                val itemSlotWidth = slotWidth.roundToInt()
                itemPlaceables = itemMeasurables.map {
                    it.measure(constraints.copy(minWidth = itemSlotWidth, maxWidth = itemSlotWidth))
                }
            } else {
                // The bar takes the width its content actually needs rather than the width the parent happens
                // to offer. The center counts as one more slot: hugging, it is just another icon, so it gets
                // the same slot width and the same gaps as the items.
                itemPlaceables = itemMeasurables.map {
                    it.measure(constraints.copy(minWidth = 0, maxWidth = Constraints.Infinity))
                }
                val slotCount = (itemCount + if (centerPlaceable != null) 1 else 0).coerceAtLeast(1)
                val naturalSlot = maxOf(itemPlaceables.maxOfOrNull { it.width } ?: 0, centerWidth)
                width = (naturalSlot * slotCount * HUG_WIDTH_SCALE).roundToInt()
                    .coerceAtMost(constraints.maxWidth)
                slotWidth = width.toFloat() / slotCount
                centerX = (slotWidth * leftCount + (slotWidth - centerWidth) / 2f).roundToInt()
                rightStart = slotWidth * (leftCount + 1)
            }

            fun itemX(index: Int): Float = if (index < leftCount) {
                slotWidth * index
            } else {
                rightStart + slotWidth * (index - leftCount)
            }

            val height = maxOf(
                itemPlaceables.maxOfOrNull { it.height } ?: 0,
                centerPlaceable?.height ?: 0
            )

            val indicatorWidth = (slotWidth + overshootPx * 2).roundToInt().coerceAtLeast(0)
            val indicatorPlaceable = subcompose(NavBarSlot.Indicator) {
                Box(modifier = Modifier.background(color = indicatorColor, shape = indicatorShape))
            }.first().measure(Constraints.fixed(indicatorWidth, height))

            val lastIndex = (itemCount - 1).coerceAtLeast(0)

            layout(width, height) {
                val lowerIndex = floor(animatedIndex.value).toInt().coerceIn(0, lastIndex)
                val upperIndex = (lowerIndex + 1).coerceAtMost(lastIndex)
                val fraction = (animatedIndex.value - lowerIndex).coerceIn(0f, 1f)

                val indicatorX = lerp(itemX(lowerIndex), itemX(upperIndex), fraction)
                indicatorPlaceable.place((indicatorX - overshootPx).roundToInt(), 0)

                itemPlaceables.forEachIndexed { index, placeable ->
                    val slotPadding = (slotWidth - placeable.width) / 2f
                    placeable.place(
                        (itemX(index) + slotPadding).roundToInt(),
                        (height - placeable.height) / 2
                    )
                }

                centerPlaceable?.place(centerX, (height - centerPlaceable.height) / 2)
            }
        }
    }
}

@Composable
fun PolkadotNavigationBarItem(
    selected: Boolean,
    onClick: () -> Unit,
    icon: ImageVector,
    label: String?,
    hasNotification: Boolean = false
) {
    PolkadotNavigationBarItem(
        selected = selected,
        onClick = onClick,
        label = label,
        hasNotification = hasNotification
    ) { contentColor ->
        NovaIcon(
            modifier = Modifier.fillMaxSize(),
            imageVector = icon,
            tint = contentColor
        )
    }
}

@Composable
fun PolkadotNavigationBarItem(
    selected: Boolean,
    onClick: () -> Unit,
    label: String?,
    hasNotification: Boolean = false,
    icon: @Composable (contentColor: Color) -> Unit
) {
    val targetColor = if (selected) PolkadotTheme.colors.fg.primary else PolkadotTheme.colors.fg.secondary
    val contentColor by animateColorAsState(
        targetValue = targetColor,
        label = "PolkadotNavigationBarItemContentColor"
    )

    Column(
        modifier = Modifier
            .clickable(
                onClick = onClick,
                indication = null,
                interactionSource = remember { MutableInteractionSource() }
            )
            .padding(
                horizontal = PolkadotTheme.spacings.small,
                vertical = PolkadotTheme.spacings.tiny
            ),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center
    ) {
        Box {
            Box(
                modifier = Modifier
                    .padding(horizontal = PolkadotTheme.spacings.small)
                    .size(IconSize)
            ) {
                icon(contentColor)
            }

            NotificationDot(
                visible = hasNotification,
                modifier = Modifier.align(Alignment.TopEnd)
            )
        }
        if (label != null) {
            VerticalSpacer { extraTiny }
            NovaText(
                text = label,
                style = PolkadotTheme.typography.label.smallEmphasized,
                color = contentColor
            )
        }
    }
}

@Composable
private fun NotificationDot(
    visible: Boolean,
    modifier: Modifier = Modifier
) {
    AnimatedVisibility(
        modifier = modifier,
        visible = visible,
        enter = scaleIn() + fadeIn(),
        exit = scaleOut() + fadeOut()
    ) {
        Box(
            modifier = Modifier
                .size(NotificationDotSize)
                .background(
                    color = PolkadotTheme.colors.bg.status.warning,
                    shape = CircleShape
                )
        )
    }
}

@Preview
@Composable
private fun PolkadotNavigationBarPreview() {
    val tabs = listOf(
        "Chats" to NovaIcons.ChatFilled,
        "Pocket" to NovaIcons.MoneyFilled,
        "Explore" to NovaIcons.Search,
        "Settings" to NovaIcons.Settings
    )
    var selectedIndex by remember { mutableIntStateOf(0) }

    PolkadotTheme {
        Box(
            modifier = Modifier.background(Color.Black)
        ) {
            PolkadotNavigationBar(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(PolkadotTheme.spacings.extraMedium),
                selectedIndex = selectedIndex,
                itemCount = tabs.size
            ) {
                tabs.forEachIndexed { index, (title, icon) ->
                    PolkadotNavigationBarItem(
                        selected = selectedIndex == index,
                        onClick = { selectedIndex = index },
                        icon = icon,
                        label = title,
                        hasNotification = title == "Settings"
                    )
                }
            }
        }
    }
}

@Preview
@Composable
private fun PolkadotNavigationBarCenterPreview() {
    val tabs = listOf(
        "Chats" to NovaIcons.ChatFilled,
        "Pocket" to NovaIcons.MoneyFilled,
        "Explore" to NovaIcons.Search,
        "Settings" to NovaIcons.Settings
    )
    var selectedIndex by remember { mutableIntStateOf(0) }

    PolkadotTheme {
        Box(
            modifier = Modifier.background(Color.Black)
        ) {
            PolkadotNavigationBar(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(PolkadotTheme.spacings.extraMedium),
                selectedIndex = selectedIndex,
                itemCount = tabs.size,
                centerContent = {
                    Box(
                        modifier = Modifier
                            .padding(horizontal = PolkadotTheme.spacings.small)
                            .size(IconSize)
                            .background(
                                color = PolkadotTheme.colors.bg.surface.nested,
                                shape = CircleShape
                            )
                    )
                }
            ) {
                tabs.forEachIndexed { index, (title, icon) ->
                    PolkadotNavigationBarItem(
                        selected = selectedIndex == index,
                        onClick = { selectedIndex = index },
                        icon = icon,
                        label = title,
                        hasNotification = title == "Settings"
                    )
                }
            }
        }
    }
}

@Preview
@Composable
private fun PolkadotNavigationBarNoLabelsPreview() {
    val tabs = listOf(
        NovaIcons.ChatFilled,
        NovaIcons.MoneyFilled,
        NovaIcons.Settings
    )
    var selectedIndex by remember { mutableIntStateOf(0) }

    PolkadotTheme {
        Box(
            modifier = Modifier.background(Color.Black)
        ) {
            PolkadotNavigationBar(
                modifier = Modifier.padding(PolkadotTheme.spacings.extraMedium),
                selectedIndex = selectedIndex,
                itemCount = tabs.size,
                fillWidth = false,
                centerContent = {
                    NovaIcon(
                        modifier = Modifier
                            .padding(horizontal = PolkadotTheme.spacings.medium)
                            .size(IconSize),
                        imageVector = NovaIcons.Search,
                        tint = PolkadotTheme.colors.fg.secondary
                    )
                }
            ) {
                tabs.forEachIndexed { index, icon ->
                    PolkadotNavigationBarItem(
                        selected = selectedIndex == index,
                        onClick = { selectedIndex = index },
                        icon = icon,
                        label = null
                    )
                }
            }
        }
    }
}

@Preview
@Composable
private fun PolkadotNavigationBarOddCenterPreview() {
    val tabs = listOf(
        "Chats" to NovaIcons.ChatFilled,
        "Pocket" to NovaIcons.MoneyFilled,
        "Settings" to NovaIcons.Settings
    )
    var selectedIndex by remember { mutableIntStateOf(0) }

    PolkadotTheme {
        Box(
            modifier = Modifier.background(Color.Black)
        ) {
            PolkadotNavigationBar(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(PolkadotTheme.spacings.extraMedium),
                selectedIndex = selectedIndex,
                itemCount = tabs.size,
                centerContent = {
                    Box(
                        modifier = Modifier
                            .padding(horizontal = PolkadotTheme.spacings.small)
                            .size(IconSize)
                            .background(
                                color = PolkadotTheme.colors.bg.surface.nested,
                                shape = CircleShape
                            )
                    )
                }
            ) {
                tabs.forEachIndexed { index, (title, icon) ->
                    PolkadotNavigationBarItem(
                        selected = selectedIndex == index,
                        onClick = { selectedIndex = index },
                        icon = icon,
                        label = title,
                        hasNotification = title == "Settings"
                    )
                }
            }
        }
    }
}
