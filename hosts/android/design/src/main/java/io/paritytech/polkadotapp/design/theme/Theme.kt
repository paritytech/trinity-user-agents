package io.paritytech.polkadotapp.design.theme

import androidx.activity.compose.LocalActivity
import androidx.compose.foundation.LocalIndication
import androidx.compose.foundation.text.selection.LocalTextSelectionColors
import androidx.compose.foundation.text.selection.TextSelectionColors
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.LocalTextStyle
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Typography
import androidx.compose.material3.ripple
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalInspectionMode
import androidx.core.view.WindowCompat
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dagger.hilt.android.EntryPointAccessors
import io.paritytech.polkadotapp.design.configs.materialShapes
import io.paritytech.polkadotapp.designsystem.borders.LocalPolkadotBorders
import io.paritytech.polkadotapp.designsystem.borders.PolkadotBorders
import io.paritytech.polkadotapp.designsystem.borders.PolkadotDefaultBorders
import io.paritytech.polkadotapp.designsystem.colors.LocalPolkadotColors
import io.paritytech.polkadotapp.designsystem.colors.PolkadotColorsPalette
import io.paritytech.polkadotapp.designsystem.radii.LocalPolkadotRadii
import io.paritytech.polkadotapp.designsystem.radii.PolkadotDefaultRadii
import io.paritytech.polkadotapp.designsystem.radii.PolkadotRadii
import io.paritytech.polkadotapp.designsystem.shapes.LocalPolkadotShapes
import io.paritytech.polkadotapp.designsystem.shapes.PolkadotDefaultShapes
import io.paritytech.polkadotapp.designsystem.shapes.PolkadotShapes
import io.paritytech.polkadotapp.designsystem.spacings.LocalPolkadotSpacings
import io.paritytech.polkadotapp.designsystem.spacings.PolkadotDefaultSpacings
import io.paritytech.polkadotapp.designsystem.spacings.PolkadotSpacings
import io.paritytech.polkadotapp.designsystem.themes.PolkadotAppTheme
import io.paritytech.polkadotapp.designsystem.typography.LocalPolkadotTypography
import io.paritytech.polkadotapp.designsystem.typography.PolkadotTypography
import kotlinx.coroutines.flow.flowOf

@Composable
fun PolkadotTheme(
    content: @Composable () -> Unit
) {
    val selector = rememberAppThemeSelector()
    val theme by selector.selectedTheme.collectAsStateWithLifecycle(PolkadotAppTheme.DEFAULT)

    val activity = LocalActivity.current
    if (activity != null) {
        val systemBarsAppearance = rememberSystemBarsAppearanceStateHolder()
        val forceDarkBackground by systemBarsAppearance.forceDarkBackground.collectAsStateWithLifecycle()
        val lightSystemBarIcons = !theme.isDark && !forceDarkBackground

        SideEffect {
            val controller = WindowCompat.getInsetsController(activity.window, activity.window.decorView)
            controller.isAppearanceLightStatusBars = lightSystemBarIcons
            controller.isAppearanceLightNavigationBars = lightSystemBarIcons
        }
    }

    PolkadotTheme(theme = theme, content = content)
}

@Composable
fun PolkadotTheme(
    theme: PolkadotAppTheme,
    content: @Composable () -> Unit
) {
    val colors: PolkadotColorsPalette = remember(theme) { theme.colors() }
    val typography: PolkadotTypography = remember(theme) { theme.typography() }

    val spacings: PolkadotSpacings = remember { PolkadotDefaultSpacings() }
    val radii: PolkadotRadii = remember { PolkadotDefaultRadii() }
    val shapes: PolkadotShapes = remember { PolkadotDefaultShapes() }
    val borders: PolkadotBorders = remember { PolkadotDefaultBorders() }

    val textSelectionColors = remember(colors) {
        TextSelectionColors(
            handleColor = colors.fg.tertiary,
            backgroundColor = colors.fg.primary.copy(alpha = 0.34f)
        )
    }
    val colorScheme = remember(colors) { colors.toMaterialColorScheme() }
    val materialTypography = remember(typography) { typography.toMaterialTypography() }

    MaterialTheme(
        colorScheme = colorScheme,
        typography = materialTypography,
        shapes = materialShapes()
    ) {
        CompositionLocalProvider(
            LocalPolkadotColors provides colors,
            LocalPolkadotTypography provides typography,
            LocalPolkadotSpacings provides spacings,
            LocalPolkadotRadii provides radii,
            LocalPolkadotShapes provides shapes,
            LocalPolkadotBorders provides borders,
            LocalIndication provides ripple(color = colors.fg.primary),
            LocalContentColor provides colors.fg.primary,
            LocalTextStyle provides typography.body.medium,
            LocalTextSelectionColors provides textSelectionColors
        ) {
            content()
        }
    }
}

@Composable
fun ForceDarkBackgroundSystemBarsEffect() {
    val systemBarsAppearance = rememberSystemBarsAppearanceStateHolder()

    DisposableEffect(systemBarsAppearance) {
        systemBarsAppearance.acquireForceDarkBackground()

        onDispose {
            systemBarsAppearance.releaseForceDarkBackground()
        }
    }
}

@Composable
private fun rememberSystemBarsAppearanceStateHolder(): SystemBarsAppearanceStateHolder {
    if (LocalInspectionMode.current) return remember { SystemBarsAppearanceStateHolder() }

    val context = LocalContext.current
    return remember(context) {
        EntryPointAccessors
            .fromApplication(context.applicationContext, AppThemeSelectorEntryPoint::class.java)
            .systemBarsAppearanceStateHolder()
    }
}

@Composable
private fun rememberAppThemeSelector(): AppThemeSelector {
    if (LocalInspectionMode.current) return DefaultAppThemeSelector

    val context = LocalContext.current
    return remember(context) {
        EntryPointAccessors
            .fromApplication(context.applicationContext, AppThemeSelectorEntryPoint::class.java)
            .appThemeSelector()
    }
}

private val PolkadotAppTheme.isDark: Boolean
    get() = when (this) {
        PolkadotAppTheme.BerlinNight -> true
        PolkadotAppTheme.BerlinDay,
        PolkadotAppTheme.Tokyo,
        PolkadotAppTheme.Lisbon,
        PolkadotAppTheme.Malta -> false
    }

private object DefaultAppThemeSelector : AppThemeSelector {
    override val selectedTheme = flowOf(PolkadotAppTheme.DEFAULT)
    override fun select(theme: PolkadotAppTheme) = Unit
}

object PolkadotTheme {
    val colors: PolkadotColorsPalette
        @Composable
        @ReadOnlyComposable
        get() = LocalPolkadotColors.current

    val typography: PolkadotTypography
        @Composable
        @ReadOnlyComposable
        get() = LocalPolkadotTypography.current

    val spacings: PolkadotSpacings
        @Composable
        @ReadOnlyComposable
        get() = LocalPolkadotSpacings.current

    val radii: PolkadotRadii
        @Composable
        @ReadOnlyComposable
        get() = LocalPolkadotRadii.current

    val shapes: PolkadotShapes
        @Composable
        @ReadOnlyComposable
        get() = LocalPolkadotShapes.current

    val borders: PolkadotBorders
        @Composable
        @ReadOnlyComposable
        get() = LocalPolkadotBorders.current
}

private fun PolkadotColorsPalette.toMaterialColorScheme(): ColorScheme = ColorScheme(
    primary = bg.action.primary,
    onPrimary = fg.primaryInverted,
    primaryContainer = bg.surface.container,
    onPrimaryContainer = fg.primary,
    inversePrimary = fg.primaryInverted,
    secondary = bg.surface.container,
    onSecondary = fg.secondary,
    secondaryContainer = bg.surface.nested,
    onSecondaryContainer = fg.primary,
    tertiary = bg.action.tertiary,
    onTertiary = fg.tertiary,
    tertiaryContainer = bg.surface.container,
    onTertiaryContainer = fg.tertiary,
    background = bg.surface.main,
    onBackground = fg.primary,
    surface = bg.surface.main,
    onSurface = fg.primary,
    surfaceVariant = bg.surface.container,
    onSurfaceVariant = fg.primary,
    surfaceTint = fg.tertiary,
    inverseSurface = bg.surface.containerInverted,
    inverseOnSurface = fg.primaryInverted,
    error = fg.error,
    onError = fg.primaryInverted,
    errorContainer = bg.status.error,
    onErrorContainer = fg.primary,
    outline = stroke.primary,
    outlineVariant = stroke.secondary,
    scrim = bg.surface.overlay,
    surfaceBright = bg.surface.main,
    surfaceDim = bg.surface.main,
    surfaceContainer = bg.surface.container,
    surfaceContainerHigh = bg.surface.container,
    surfaceContainerHighest = bg.surface.container,
    surfaceContainerLow = bg.surface.nested,
    surfaceContainerLowest = bg.surface.nested
)

private fun PolkadotTypography.toMaterialTypography(): Typography = Typography(
    displayLarge = display.large,
    displayMedium = display.medium,
    displaySmall = display.small,
    headlineLarge = headline.large,
    headlineMedium = headline.medium,
    headlineSmall = headline.small,
    titleLarge = title.large,
    titleMedium = title.medium,
    titleSmall = title.small,
    bodyLarge = body.large,
    bodyMedium = body.medium,
    bodySmall = body.small,
    labelLarge = label.medium,
    labelMedium = label.medium,
    labelSmall = label.small
)
