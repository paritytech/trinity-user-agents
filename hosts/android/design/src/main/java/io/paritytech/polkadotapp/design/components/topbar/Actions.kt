package io.paritytech.polkadotapp.design.components.topbar

import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.remember
import androidx.compose.ui.graphics.vector.ImageVector
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowLeft

@Composable
fun rememberTopBarAction(
    action: () -> Unit,
    icon: ImageVector = NovaIcons.ArrowLeft
): TopBarAction = remember { RealTopBarAction(icon, action) }

interface TopBarAction {
    val icon: ImageVector
    val action: () -> Unit
}

@Immutable
private data class RealTopBarAction(
    override val icon: ImageVector,
    override val action: () -> Unit
) : TopBarAction
