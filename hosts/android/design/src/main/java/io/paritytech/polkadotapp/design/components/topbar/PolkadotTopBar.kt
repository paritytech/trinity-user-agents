package io.paritytech.polkadotapp.design.components.topbar

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.dp
import androidx.compose.ui.util.fastForEach
import io.paritytech.polkadotapp.design.components.avatar.AvatarUiModel
import io.paritytech.polkadotapp.design.components.avatar.Mock
import io.paritytech.polkadotapp.design.components.avatar.PolkadotAvatar
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonShape
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.icon.PolkadotIconButton
import io.paritytech.polkadotapp.design.components.button.icon.PolkadotIconButtonSize
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowLeft
import io.paritytech.polkadotapp.design.components.icon.vectors.CallFilled
import io.paritytech.polkadotapp.design.components.icon.vectors.More
import io.paritytech.polkadotapp.design.components.icon.vectors.VideocamFilled
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.persistentListOf

const val MAX_TOP_BAR_ACTIONS = 3

private val LeadingContentSize = 40.dp

private val TopBarHeight = 64.dp

enum class TopBarTitleAlignment { Start, Center }

enum class TopBarTitleSize { Standard, Medium, Large }

@Composable
fun PolkadotTopBar(
    modifier: Modifier = Modifier,
    navigationAction: TopBarAction? = null,
    title: String? = null,
    subtitle: String? = null,
    titleAlignment: TopBarTitleAlignment = TopBarTitleAlignment.Start,
    titleSize: TopBarTitleSize = TopBarTitleSize.Standard,
    actions: ImmutableList<TopBarAction> = persistentListOf(),
    leadingContent: (@Composable () -> Unit)? = null,
    content: (@Composable () -> Unit)? = null,
) {
    require(actions.size <= MAX_TOP_BAR_ACTIONS) { "Top bar supports at most $MAX_TOP_BAR_ACTIONS actions" }

    val titleStyle = when (titleSize) {
        TopBarTitleSize.Standard -> PolkadotTheme.typography.title.medium
        TopBarTitleSize.Medium -> PolkadotTheme.typography.title.large
        TopBarTitleSize.Large -> PolkadotTheme.typography.headline.small
    }
    val hasLeading = navigationAction != null || leadingContent != null
    val edgePadding = if (content != null) PolkadotTheme.spacings.mediumIncreased else PolkadotTheme.spacings.tiny
    val innerModifier = Modifier
        .statusBarsPadding()
        .fillMaxWidth()
        .heightIn(min = TopBarHeight)
        .padding(
            start = if (content == null && !hasLeading) PolkadotTheme.spacings.mediumIncreased else edgePadding,
            end = edgePadding,
            top = PolkadotTheme.spacings.small,
            bottom = PolkadotTheme.spacings.small,
        )

    Box(modifier = modifier) {
        if (content == null && titleAlignment == TopBarTitleAlignment.Center) {
            CenteredTopBar(
                modifier = innerModifier,
                navigationAction = navigationAction,
                leadingContent = leadingContent,
                title = title,
                subtitle = subtitle,
                titleStyle = titleStyle,
                actions = actions,
            )
        } else {
            StartTopBar(
                modifier = innerModifier,
                navigationAction = navigationAction,
                leadingContent = leadingContent,
                title = title,
                subtitle = subtitle,
                titleStyle = titleStyle,
                actions = actions,
                content = content,
            )
        }
    }
}

@Composable
private fun CenteredTopBar(
    navigationAction: TopBarAction?,
    leadingContent: (@Composable () -> Unit)?,
    title: String?,
    subtitle: String?,
    titleStyle: TextStyle,
    actions: ImmutableList<TopBarAction>,
    modifier: Modifier = Modifier,
) {
    // Reserve equal side gutters (= the wider side) so the title is centered to the bar itself,
    // not to the leftover gap between an asymmetric leading/trailing (e.g. back + 2 actions).
    Layout(
        modifier = modifier,
        contents = listOf(
            { LeadingGroup(navigationAction, leadingContent) },
            {
                TitleColumn(
                    title = title,
                    subtitle = subtitle,
                    titleStyle = titleStyle,
                    textAlign = TextAlign.Center,
                    horizontalAlignment = Alignment.CenterHorizontally,
                )
            },
            { if (actions.isNotEmpty()) TopBarActions(actions) },
        ),
    ) { (startMeasurables, centerMeasurables, endMeasurables), constraints ->
        val looseConstraints = constraints.copy(minWidth = 0)
        val start = startMeasurables.map { it.measure(looseConstraints) }
        val end = endMeasurables.map { it.measure(looseConstraints) }

        val sideWidth = maxOf(start.maxOfOrNull { it.width } ?: 0, end.maxOfOrNull { it.width } ?: 0)
        val centerMaxWidth = (constraints.maxWidth - sideWidth * 2).coerceAtLeast(0)
        val center = centerMeasurables.map {
            it.measure(Constraints(maxWidth = centerMaxWidth, maxHeight = constraints.maxHeight))
        }

        val height = maxOf(
            start.maxOfOrNull { it.height } ?: 0,
            end.maxOfOrNull { it.height } ?: 0,
            center.maxOfOrNull { it.height } ?: 0,
            constraints.minHeight,
        )
        layout(constraints.maxWidth, height) {
            start.forEach { it.placeRelative(0, (height - it.height) / 2) }
            end.forEach { it.placeRelative(constraints.maxWidth - it.width, (height - it.height) / 2) }

            val centerTotalWidth = center.sumOf { it.width }
            var centerX = (constraints.maxWidth - centerTotalWidth) / 2
            center.forEach {
                it.placeRelative(centerX, (height - it.height) / 2)
                centerX += it.width
            }
        }
    }
}

@Composable
private fun StartTopBar(
    navigationAction: TopBarAction?,
    leadingContent: (@Composable () -> Unit)?,
    title: String?,
    subtitle: String?,
    titleStyle: TextStyle,
    actions: ImmutableList<TopBarAction>,
    content: (@Composable () -> Unit)?,
    modifier: Modifier = Modifier,
) {
    Row(
        modifier = modifier,
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.mediumIncreased),
    ) {
        if (navigationAction != null || leadingContent != null) {
            LeadingGroup(navigationAction, leadingContent)
        }

        // The fill slot (e.g. a search field) takes the flexible middle; the bar owns the weight
        // so callers never deal with RowScope.
        if (content != null) {
            Box(modifier = Modifier.weight(1f)) {
                content()
            }
        } else {
            TitleColumn(
                modifier = Modifier.weight(1f),
                title = title,
                subtitle = subtitle,
                titleStyle = titleStyle,
                textAlign = TextAlign.Start,
                horizontalAlignment = Alignment.Start,
            )
        }

        if (actions.isNotEmpty()) {
            TopBarActions(actions)
        }
    }
}

@Composable
private fun LeadingGroup(
    navigationAction: TopBarAction?,
    leadingContent: (@Composable () -> Unit)?,
) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small),
    ) {
        if (navigationAction != null) {
            TopBarActionButton(
                icon = navigationAction.icon,
                onClick = navigationAction.action,
            )
        }

        if (leadingContent != null) {
            Box(
                modifier = Modifier.size(LeadingContentSize),
                contentAlignment = Alignment.Center,
            ) {
                leadingContent()
            }
        }
    }
}

@Composable
private fun TitleColumn(
    title: String?,
    subtitle: String?,
    titleStyle: TextStyle,
    textAlign: TextAlign,
    horizontalAlignment: Alignment.Horizontal,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier = modifier,
        horizontalAlignment = horizontalAlignment,
    ) {
        if (title != null) {
            NovaText(
                text = title,
                style = titleStyle,
                color = PolkadotTheme.colors.fg.primary,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                textAlign = textAlign,
            )
        }
        if (subtitle != null) {
            NovaText(
                text = subtitle,
                style = PolkadotTheme.typography.body.small,
                color = PolkadotTheme.colors.fg.tertiary,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                textAlign = textAlign,
            )
        }
    }
}

@Composable
private fun TopBarActions(actions: ImmutableList<TopBarAction>) {
    // Single Row so the parent's itemSpacing never separates individual action buttons —
    // Figma keeps trailing icon buttons flush (their 12dp internal padding is the only gap).
    Row(verticalAlignment = Alignment.CenterVertically) {
        actions.fastForEach { action ->
            TopBarActionButton(
                icon = action.icon,
                onClick = action.action,
            )
        }
    }
}

@Composable
private fun TopBarActionButton(
    icon: ImageVector,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    PolkadotIconButton(
        modifier = modifier,
        icon = icon,
        onClick = onClick,
        style = PolkadotButtonStyle.ghost(),
        size = PolkadotIconButtonSize.mediumIncreased(),
        shape = PolkadotButtonShape.pill,
    )
}

@Preview(showBackground = true, backgroundColor = 0xFF000000)
@Composable
private fun PolkadotTopBarPreview() {
    PolkadotTheme {
        Column {
            // Big title (large heading), start-aligned
            PolkadotTopBar(
                title = "Label",
                titleSize = TopBarTitleSize.Large,
                actions = persistentListOf(rememberTopBarAction(action = {}, icon = NovaIcons.More)),
            )

            PolkadotTopBar(
                navigationAction = rememberTopBarAction(action = {}, icon = NovaIcons.ArrowLeft),
                title = "Label",
                titleAlignment = TopBarTitleAlignment.Center,
                titleSize = TopBarTitleSize.Medium,
            )

            // Back, centered title + subtitle + action
            PolkadotTopBar(
                navigationAction = rememberTopBarAction(action = {}, icon = NovaIcons.ArrowLeft),
                title = "Label",
                subtitle = "Supporting text",
                titleAlignment = TopBarTitleAlignment.Center,
                actions = persistentListOf(rememberTopBarAction(action = {}, icon = NovaIcons.More)),
            )

            // Back, centered title + subtitle, no actions
            PolkadotTopBar(
                navigationAction = rememberTopBarAction(action = {}, icon = NovaIcons.ArrowLeft),
                title = "LongLabelWithManyWorks",
                subtitle = "Supporting text",
                titleAlignment = TopBarTitleAlignment.Center,
                actions = persistentListOf(
                    rememberTopBarAction(action = {}, icon = NovaIcons.CallFilled),
                    rememberTopBarAction(action = {}, icon = NovaIcons.VideocamFilled),
                    rememberTopBarAction(action = {}, icon = NovaIcons.More),
                ),
            )

            // Chat: back + leading avatar + title/subtitle + actions
            PolkadotTopBar(
                navigationAction = rememberTopBarAction(action = {}, icon = NovaIcons.ArrowLeft),
                title = "Label",
                subtitle = "Supporting text",
                actions = persistentListOf(
                    rememberTopBarAction(action = {}, icon = NovaIcons.VideocamFilled),
                    rememberTopBarAction(action = {}, icon = NovaIcons.More),
                ),
                leadingContent = {
                    PolkadotAvatar(
                        model = AvatarUiModel.Mock.fromName("Andrey"),
                        modifier = Modifier.fillMaxSize(),
                    )
                },
            )

            // Search via the fill content slot (back-arrow leading per the Figma "Search" variant)
            PolkadotTopBar {
                PolkadotSearchField(
                    modifier = Modifier.fillMaxWidth(),
                    value = "Input text",
                    onValueChange = {},
                    onClear = {},
                    placeholder = "Search",
                    leadingIcon = NovaIcons.ArrowLeft,
                    onLeadingClick = {},
                )
            }

            // Back only
            PolkadotTopBar(
                navigationAction = rememberTopBarAction(action = {}, icon = NovaIcons.ArrowLeft),
            )
        }
    }
}
