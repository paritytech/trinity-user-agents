package io.paritytech.polkadotapp.common.presentation.statusBar

import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.State
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.staticCompositionLocalOf
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class StatusBarTitleHolder @Inject constructor() {
    private val shownTitles = mutableStateListOf<State<@Composable () -> Unit>>()

    val isShown: Boolean
        get() = shownTitles.isNotEmpty()

    @Composable
    fun Title() {
        shownTitles.lastOrNull()?.let { title ->
            key(title) { title.value() }
        }
    }

    fun onTitleShown(title: State<@Composable () -> Unit>) {
        shownTitles += title
    }

    fun onTitleGone(title: State<@Composable () -> Unit>) {
        shownTitles.remove(title)
    }
}

val LocalStatusBarTitle = staticCompositionLocalOf<StatusBarTitleHolder?> { null }

@Composable
fun StatusBarTitle(content: @Composable () -> Unit) {
    val holder = LocalStatusBarTitle.current
    val title = rememberUpdatedState(content)

    DisposableEffect(holder, title) {
        holder?.onTitleShown(title)
        onDispose { holder?.onTitleGone(title) }
    }
}
