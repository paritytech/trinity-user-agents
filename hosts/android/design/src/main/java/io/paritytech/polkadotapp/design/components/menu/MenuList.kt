package io.paritytech.polkadotapp.design.components.menu

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.LocalTextStyle
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

private val ContainersSize = 32.dp

@Composable
fun PolkadotMenuList(
    modifier: Modifier = Modifier,
    headerText: String? = null,
    content: @Composable PolkadotMenuListScope.() -> Unit
) {
    Column(modifier = modifier) {
        headerText?.let {
            PolkadotMenuListHeader(
                modifier = Modifier.padding(horizontal = PolkadotTheme.spacings.tiny),
                text = it
            )
            VerticalSpacer { extraMedium }
        }

        // Entries round themselves off with the inner radius and the group clip supplies the outer corners.
        // Rounding per index instead would mean collecting every entry into a list before emitting any of
        // them, and an entry whose visibility flips after the first composition would then never be emitted.
        Column(
            modifier = Modifier.clip(RoundedCornerShape(PolkadotTheme.radii.medium)),
            verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.extraTiny)
        ) {
            PolkadotMenuListScope.content()
        }
    }
}

object PolkadotMenuListScope

@Composable
fun PolkadotMenuListItem(
    modifier: Modifier = Modifier,
    leading: (@Composable BoxScope.() -> Unit)? = null,
    trailing: (@Composable () -> Unit)? = null,
    title: @Composable () -> Unit,
    onClick: () -> Unit,
    description: (@Composable () -> Unit)? = null,
) {
    PolkadotMenuListItemInternal(
        modifier = modifier,
        shape = PolkadotTheme.shapes.medium,
        leading = leading,
        trailing = trailing,
        title = title,
        description = description,
        onClick = onClick
    )
}

@Composable
fun PolkadotMenuListScope.PolkadotMenuListItem(
    leading: (@Composable BoxScope.() -> Unit)? = null,
    trailing: (@Composable () -> Unit)? = null,
    title: @Composable () -> Unit,
    description: (@Composable () -> Unit)? = null,
    onClick: () -> Unit,
) {
    PolkadotMenuListItemInternal(
        modifier = Modifier,
        shape = menuItemShape(),
        leading = leading,
        trailing = trailing,
        title = title,
        description = description,
        onClick = onClick
    )
}

/**
 * Emits arbitrary content as a list entry. The [Shape] handed to [content] is the one a row would use, so a
 * custom widget rounds off against its neighbours the same way.
 */
@Composable
fun PolkadotMenuListScope.PolkadotMenuListCustomItem(content: @Composable (Shape) -> Unit) {
    content(menuItemShape())
}

@Composable
private fun PolkadotMenuListItemInternal(
    modifier: Modifier,
    shape: Shape,
    leading: (@Composable BoxScope.() -> Unit)?,
    trailing: (@Composable () -> Unit)?,
    title: @Composable () -> Unit,
    description: (@Composable () -> Unit)?,
    onClick: () -> Unit,
) {
    PolkadotSurface(
        modifier = modifier,
        shape = shape,
        color = PolkadotTheme.colors.bg.surface.container,
        onClick = onClick
    ) {
        Row(
            modifier = Modifier.padding(PolkadotTheme.spacings.mediumIncreased),
            horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.mediumIncreased),
            verticalAlignment = Alignment.CenterVertically
        ) {
            leading?.let {
                Box(
                    modifier = Modifier.size(ContainersSize),
                ) {
                    it()
                }
            }

            Column(
                modifier = Modifier
                    .weight(1f)
                    .defaultMinSize(minHeight = ContainersSize),
                verticalArrangement = Arrangement.Center
            ) {
                CompositionLocalProvider(
                    LocalContentColor provides PolkadotTheme.colors.fg.primary,
                    LocalTextStyle provides PolkadotTheme.typography.body.large
                ) {
                    title()
                }
                description?.let {
                    CompositionLocalProvider(
                        LocalContentColor provides PolkadotTheme.colors.fg.secondary,
                        LocalTextStyle provides PolkadotTheme.typography.body.smallEmphasized
                    ) {
                        it()
                    }
                }
            }

            trailing?.invoke()
        }
    }
}

@Composable
private fun menuItemShape(): Shape = RoundedCornerShape(PolkadotTheme.radii.extraSmall)
