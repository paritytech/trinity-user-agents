package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.image.NovaAsyncImage
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.components.text.NovaTextField
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_api.model.JsAlignment
import io.paritytech.polkadotapp.feature_products_api.model.JsArrangement
import io.paritytech.polkadotapp.feature_products_api.model.JsButtonVariant
import io.paritytech.polkadotapp.feature_products_api.model.JsEffect
import io.paritytech.polkadotapp.feature_products_api.model.JsHorizontalAlignment
import io.paritytech.polkadotapp.feature_products_api.model.JsImageFit
import io.paritytech.polkadotapp.feature_products_api.model.JsTypographyStyle
import io.paritytech.polkadotapp.feature_products_api.model.JsUiEvent
import io.paritytech.polkadotapp.feature_products_api.model.JsVerticalAlignment
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget

/**
 * CompositionLocal for event dispatcher, allowing nested widgets to dispatch events.
 */

/**
 * Main entry point for rendering a JsWidget tree to Compose UI.
 * @param widget The widget tree to render
 * @param eventDispatcher Handler for interactive events (onClick, onValueChange)
 */
@Composable
fun JsWidgetRenderer(
    widget: JsWidget,
    modifier: Modifier = Modifier,
    jsEventHandler: JsUiEventHandler,
) {
    CompositionLocalProvider(LocalJsEventHandler provides jsEventHandler) {
        JsWidgetContent(widget, modifier)
    }
}

@Composable
private fun JsWidgetContent(
    widget: JsWidget,
    modifier: Modifier = Modifier,
) {
    when (widget) {
        is JsWidget.Box -> JsBoxRenderer(widget, modifier)
        is JsWidget.Column -> JsColumnRenderer(widget, modifier)
        is JsWidget.Row -> JsRowRenderer(widget, modifier)
        is JsWidget.Spacer -> JsSpacerRenderer(widget, modifier)
        is JsWidget.Text -> JsTextRenderer(widget, modifier)
        is JsWidget.Button -> JsButtonRenderer(widget, modifier)
        is JsWidget.TextField -> JsTextFieldRenderer(widget, modifier)
        is JsWidget.Image -> JsImageRenderer(widget, modifier)
        is JsWidget.Effect -> JsEffectRenderer(widget, modifier)
    }
}

@Composable
private fun JsImageRenderer(
    widget: JsWidget.Image,
    modifier: Modifier = Modifier,
) {
    val resolver = LocalJsImageResolver.current
    // Kept across a re-resolve, so a face that redraws does not blink its images away.
    var model by remember(widget.source) { mutableStateOf(resolver.resolved(widget.source)) }
    LaunchedEffect(widget.source, resolver) {
        resolver.resolve(widget.source)?.let { model = it }
    }
    val imageModifier = modifier.then(widget.modifiers.toComposeModifier())

    // An image the host cannot fetch draws as empty space.
    if (model == null) {
        Spacer(modifier = imageModifier)
    } else {
        NovaAsyncImage(
            modifier = imageModifier,
            model = model,
            contentDescription = null,
            contentScale = widget.fit.toContentScale(),
        )
    }
}

@Composable
private fun JsEffectRenderer(
    widget: JsWidget.Effect,
    modifier: Modifier = Modifier,
) {
    Box(modifier = modifier) {
        widget.children.forEach { child ->
            JsWidgetContent(widget = child)
        }
        Box(
            modifier = Modifier
                .matchParentSize()
                .background(widget.effect.toBrush(), alpha = EFFECT_ALPHA),
        )
    }
}

private const val EFFECT_ALPHA = 0.3f

private fun JsEffect.toBrush(): Brush = when (this) {
    JsEffect.RAINBOW -> Brush.linearGradient(
        listOf(
            Color(0xFFFF5E5E),
            Color(0xFFFFD35E),
            Color(0xFF5EFF8A),
            Color(0xFF5ED0FF),
            Color(0xFF8A5EFF),
            Color(0xFFFF5EDB),
        ),
    )
}

private fun JsImageFit.toContentScale(): ContentScale = when (this) {
    JsImageFit.NONE -> ContentScale.None
    JsImageFit.FILL -> ContentScale.FillBounds
    JsImageFit.COVER -> ContentScale.Crop
    JsImageFit.CONTAIN -> ContentScale.Fit
    JsImageFit.SCALE_DOWN -> ContentScale.Inside
}

@Composable
private fun JsBoxRenderer(
    widget: JsWidget.Box,
    modifier: Modifier = Modifier,
) {
    Box(
        modifier = modifier.then(widget.modifiers.toComposeModifier()),
        contentAlignment = widget.contentAlignment?.toComposeAlignment() ?: Alignment.TopStart,
    ) {
        widget.children.forEach { child ->
            JsWidgetContent(widget = child)
        }
    }
}

@Composable
private fun JsColumnRenderer(
    widget: JsWidget.Column,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier = modifier.then(widget.modifiers.toComposeModifier()),
        horizontalAlignment = widget.horizontalAlignment?.toComposeHorizontalAlignment() ?: Alignment.Start,
        verticalArrangement = widget.verticalArrangement?.toComposeVerticalArrangement() ?: Arrangement.Top,
    ) {
        widget.children.forEach { child ->
            JsWidgetContent(widget = child)
        }
    }
}

@Composable
private fun JsRowRenderer(
    widget: JsWidget.Row,
    modifier: Modifier = Modifier,
) {
    Row(
        modifier = modifier.then(widget.modifiers.toComposeModifier()),
        verticalAlignment = widget.verticalAlignment?.toComposeVerticalAlignment() ?: Alignment.Top,
        horizontalArrangement = widget.horizontalArrangement?.toComposeHorizontalArrangement() ?: Arrangement.Start,
    ) {
        widget.children.forEach { child ->
            JsWidgetContent(widget = child)
        }
    }
}

@Composable
private fun JsSpacerRenderer(
    widget: JsWidget.Spacer,
    modifier: Modifier = Modifier,
) {
    Spacer(modifier = modifier.then(widget.modifiers.toComposeModifier()))
}

@Composable
private fun JsTextRenderer(
    widget: JsWidget.Text,
    modifier: Modifier = Modifier,
) {
    val textStyle = when (widget.style) {
        JsTypographyStyle.HEADLINE_LARGE -> PolkadotTheme.typography.headline.large
        JsTypographyStyle.TITLE_MEDIUM_REGULAR -> PolkadotTheme.typography.title.medium
        JsTypographyStyle.BODY_LARGE_REGULAR -> PolkadotTheme.typography.body.large
        JsTypographyStyle.BODY_MEDIUM_REGULAR -> PolkadotTheme.typography.body.medium
        JsTypographyStyle.BODY_SMALL_REGULAR -> PolkadotTheme.typography.body.small
    }

    val textColor = widget.color?.toComposeColor() ?: PolkadotTheme.colors.fg.primary

    Text(
        text = widget.text,
        style = textStyle,
        color = textColor,
        modifier = modifier.then(widget.modifiers.toComposeModifier()),
    )
}

@Composable
private fun JsButtonRenderer(
    widget: JsWidget.Button,
    modifier: Modifier = Modifier,
) {
    val eventDispatcher = LocalJsEventHandler.current
    val onClick: () -> Unit = {
        widget.onClick?.let { actionId ->
            eventDispatcher(actionId, JsUiEvent.Type.ButtonClick)
        }
        Unit
    }

    val buttonContent: @Composable () -> Unit = {
        if (widget.loading) {
            CircularProgressIndicator(
                modifier = Modifier.size(18.dp),
                color = PolkadotTheme.colors.fg.primary,
                strokeWidth = 2.dp,
            )
        } else {
            Text(text = widget.text)
        }
    }

    val buttonModifier = modifier.then(widget.modifiers.toComposeModifier())

    when (widget.variant) {
        JsButtonVariant.PRIMARY -> {
            Button(
                onClick = onClick,
                enabled = widget.enabled && !widget.loading,
                modifier = buttonModifier,
                colors = ButtonDefaults.buttonColors(
                    containerColor = PolkadotTheme.colors.fg.staticWhite,
                    contentColor = PolkadotTheme.colors.fg.primaryInverted,
                ),
            ) {
                buttonContent()
            }
        }

        JsButtonVariant.SECONDARY -> {
            OutlinedButton(
                onClick = onClick,
                enabled = widget.enabled && !widget.loading,
                modifier = buttonModifier,
            ) {
                buttonContent()
            }
        }

        JsButtonVariant.TEXT -> {
            TextButton(
                onClick = onClick,
                enabled = widget.enabled && !widget.loading,
                modifier = buttonModifier,
            ) {
                buttonContent()
            }
        }
    }
}

@Composable
private fun JsTextFieldRenderer(
    widget: JsWidget.TextField,
    modifier: Modifier = Modifier,
) {
    val eventDispatcher = LocalJsEventHandler.current

    NovaTextField(
        value = widget.value,
        onValueChange = { newValue ->
            widget.onValueChange?.let { actionId ->
                eventDispatcher(actionId, JsUiEvent.Type.InputFieldValueChange(newValue))
            }
        },
        enabled = widget.enabled,
        label = widget.label?.let { { Text(it) } },
        placeholder = widget.placeholder?.let {
            {
                NovaText(
                    text = it,
                    style = PolkadotTheme.typography.body.large,
                    color = PolkadotTheme.colors.fg.tertiary
                )
            }
        },
        modifier = modifier
            .then(widget.modifiers.toComposeModifier())
            .fillMaxWidth(),
        singleLine = true,
    )
}

// Alignment converters

private fun JsAlignment.toComposeAlignment(): Alignment = when (this) {
    JsAlignment.TOP_START -> Alignment.TopStart
    JsAlignment.TOP_CENTER -> Alignment.TopCenter
    JsAlignment.TOP_END -> Alignment.TopEnd
    JsAlignment.CENTER_START -> Alignment.CenterStart
    JsAlignment.CENTER -> Alignment.Center
    JsAlignment.CENTER_END -> Alignment.CenterEnd
    JsAlignment.BOTTOM_START -> Alignment.BottomStart
    JsAlignment.BOTTOM_CENTER -> Alignment.BottomCenter
    JsAlignment.BOTTOM_END -> Alignment.BottomEnd
}

private fun JsHorizontalAlignment.toComposeHorizontalAlignment(): Alignment.Horizontal = when (this) {
    JsHorizontalAlignment.START -> Alignment.Start
    JsHorizontalAlignment.CENTER -> Alignment.CenterHorizontally
    JsHorizontalAlignment.END -> Alignment.End
}

private fun JsVerticalAlignment.toComposeVerticalAlignment(): Alignment.Vertical = when (this) {
    JsVerticalAlignment.TOP -> Alignment.Top
    JsVerticalAlignment.CENTER -> Alignment.CenterVertically
    JsVerticalAlignment.BOTTOM -> Alignment.Bottom
}

private fun JsArrangement.toComposeVerticalArrangement(): Arrangement.Vertical = when (this) {
    JsArrangement.START -> Arrangement.Top
    JsArrangement.END -> Arrangement.Bottom
    JsArrangement.CENTER -> Arrangement.Center
    JsArrangement.SPACE_BETWEEN -> Arrangement.SpaceBetween
    JsArrangement.SPACE_AROUND -> Arrangement.SpaceAround
    JsArrangement.SPACE_EVENLY -> Arrangement.SpaceEvenly
}

private fun JsArrangement.toComposeHorizontalArrangement(): Arrangement.Horizontal = when (this) {
    JsArrangement.START -> Arrangement.Start
    JsArrangement.END -> Arrangement.End
    JsArrangement.CENTER -> Arrangement.Center
    JsArrangement.SPACE_BETWEEN -> Arrangement.SpaceBetween
    JsArrangement.SPACE_AROUND -> Arrangement.SpaceAround
    JsArrangement.SPACE_EVENLY -> Arrangement.SpaceEvenly
}
