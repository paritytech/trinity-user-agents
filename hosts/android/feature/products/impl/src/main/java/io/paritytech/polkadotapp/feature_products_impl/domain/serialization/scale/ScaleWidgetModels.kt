package io.paritytech.polkadotapp.feature_products_impl.domain.serialization.scale

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.annotations.AsTuple
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.EnumIndex
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.serializers.BigIntegerSerializable
import kotlinx.serialization.Serializable

/**
 * SCALE-compatible Kotlin model matching the host-api SDK's CustomRendererNode schema.
 * Decoded via BinaryScale, then mapped to JsWidget for rendering.
 *
 * Enum variant indices and enum ordinals MUST match the SDK definitions in customRenderer.js.
 */
// --- Main widget tree node ---

@Serializable
sealed class ScaleWidget {
    @Serializable
    @EnumIndex(0)
    data object Nil : ScaleWidget()

    @Serializable
    @EnumIndex(1)
    data class StringNode(val value: String) : ScaleWidget()

    @Serializable
    @EnumIndex(2)
    data class Box(
        val modifiers: List<ScaleModifier>,
        val props: ScaleBoxProps,
        val children: List<ScaleWidget>,
    ) : ScaleWidget()

    @Serializable
    @EnumIndex(3)
    data class Column(
        val modifiers: List<ScaleModifier>,
        val props: ScaleColumnProps,
        val children: List<ScaleWidget>,
    ) : ScaleWidget()

    @Serializable
    @EnumIndex(4)
    data class Row(
        val modifiers: List<ScaleModifier>,
        val props: ScaleRowProps,
        val children: List<ScaleWidget>,
    ) : ScaleWidget()

    @Serializable
    @EnumIndex(5)
    data class Spacer(
        val modifiers: List<ScaleModifier>,
        val children: List<ScaleWidget>,
    ) : ScaleWidget()

    @Serializable
    @EnumIndex(6)
    data class Text(
        val modifiers: List<ScaleModifier>,
        val props: ScaleTextProps,
        val children: List<ScaleWidget>,
    ) : ScaleWidget()

    @Serializable
    @EnumIndex(7)
    data class Button(
        val modifiers: List<ScaleModifier>,
        val props: ScaleButtonProps,
        val children: List<ScaleWidget>,
    ) : ScaleWidget()

    @Serializable
    @EnumIndex(8)
    data class TextField(
        val modifiers: List<ScaleModifier>,
        val props: ScaleTextFieldProps,
        val children: List<ScaleWidget>,
    ) : ScaleWidget()
}

// --- Modifiers (Vec<Modifier> where Modifier is an enum) ---

// Enum variant order MUST match the SDK's Enum({...}) key order in customRenderer.js:
//   margin(0), padding(1), background(2), border(3), height(4), width(5),
//   minWidth(6), minHeight(7), fillWidth(8), fillHeight(9)
@Serializable
sealed class ScaleModifier {
    @Serializable
    @EnumIndex(0)
    data class Margin(val value: ScaleDimensions) : ScaleModifier()

    @Serializable
    @EnumIndex(1)
    data class Padding(val value: ScaleDimensions) : ScaleModifier()

    @Serializable
    @EnumIndex(2)
    data class Background(val value: ScaleBackground) : ScaleModifier()

    @Serializable
    @EnumIndex(3)
    data class Border(val value: ScaleBorderStyle) : ScaleModifier()

    @Serializable
    @EnumIndex(4)
    data class Height(val value: BigIntegerSerializable) : ScaleModifier()

    @Serializable
    @EnumIndex(5)
    data class Width(val value: BigIntegerSerializable) : ScaleModifier()

    @Serializable
    @EnumIndex(6)
    data class MinWidth(val value: BigIntegerSerializable) : ScaleModifier()

    @Serializable
    @EnumIndex(7)
    data class MinHeight(val value: BigIntegerSerializable) : ScaleModifier()

    @Serializable
    @EnumIndex(8)
    data class FillWidth(val value: Boolean) : ScaleModifier()

    @Serializable
    @EnumIndex(9)
    data class FillHeight(val value: Boolean) : ScaleModifier()
}

// Dimensions = Tuple(compact, compact, Option(compact), Option(compact))
// CSS-style shorthand: interpretation depends on number of values present:
//   [a, b]       → top=a, right=b, bottom=a, left=b
//   [a, b, c]    → top=a, right=b, bottom=c, left=b
//   [a, b, c, d] → top=a, right=b, bottom=c, left=d
@Serializable
@AsTuple
data class ScaleDimensions(
    val first: BigIntegerSerializable,
    val second: BigIntegerSerializable,
    val third: BigIntegerSerializable?,
    val fourth: BigIntegerSerializable?,
) {
    val top: Int get() = first.toInt()
    val end: Int get() = second.toInt()
    val bottom: Int get() = (third ?: first).toInt()
    val start: Int get() = (fourth ?: second).toInt()
}

@Serializable
data class ScaleBackground(
    val color: ScaleColorToken,
    val shape: ScaleShape?,
)

@Serializable
data class ScaleBorderStyle(
    val width: BigIntegerSerializable,
    val color: ScaleColorToken,
    val shape: ScaleShape?,
)

// --- Shape ---

@Serializable
sealed class ScaleShape {
    @Serializable
    @EnumIndex(0)
    data class Rounded(val radius: BigIntegerSerializable) : ScaleShape()

    @Serializable
    @EnumIndex(1)
    data object Circle : ScaleShape()
}

// --- Props ---

@Serializable
data class ScaleBoxProps(
    val contentAlignment: ScaleContentAlignment?,
)

@Serializable
data class ScaleColumnProps(
    val horizontalAlignment: ScaleHorizontalAlignment?,
    val verticalArrangement: ScaleArrangement?,
)

@Serializable
data class ScaleRowProps(
    val verticalAlignment: ScaleVerticalAlignment?,
    val horizontalArrangement: ScaleArrangement?,
)

@Serializable
data class ScaleTextProps(
    val style: ScaleTypographyStyle?,
    val color: ScaleColorToken?,
)

@Serializable
data class ScaleButtonProps(
    val text: String,
    val variant: ScaleButtonVariant?,
    val enabled: Boolean?,
    val loading: Boolean?,
    val clickAction: String?,
)

@Serializable
data class ScaleTextFieldProps(
    val text: String,
    val placeholder: String?,
    val label: String?,
    val enabled: Boolean?,
    val valueChangeAction: String?,
)

// --- Enums (ordinals must match SDK's Status(...) ordering) ---

// ColorToken = Status('fg.primary', 'fg.secondary', 'fg.tertiary',
//   'bg.surface.main', 'bg.surface.container', 'bg.surface.nested',
//   'fg.success', 'fg.error', 'fg.warning')
@Serializable
enum class ScaleColorToken {
    FG_PRIMARY, // 0
    FG_SECONDARY, // 1
    FG_TERTIARY, // 2
    BG_SURFACE_MAIN, // 3
    BG_SURFACE_CONTAINER, // 4
    BG_SURFACE_NESTED, // 5
    FG_SUCCESS, // 6
    FG_ERROR, // 7
    FG_WARNING, // 8
}

// TypographyStyle = Status('headline.large', 'title.medium.regular',
//   'body.large.regular', 'body.medium.regular', 'body.small.regular')
@Serializable
enum class ScaleTypographyStyle {
    HEADLINE_LARGE, // 0
    TITLE_MEDIUM_REGULAR, // 1
    BODY_LARGE_REGULAR, // 2
    BODY_MEDIUM_REGULAR, // 3
    BODY_SMALL_REGULAR, // 4
}

// ButtonVariant = Status('primary', 'secondary', 'text')
@Serializable
enum class ScaleButtonVariant {
    PRIMARY, // 0
    SECONDARY, // 1
    TEXT, // 2
}

// ContentAlignment = Status('topStart', 'topCenter', 'topEnd',
//   'centerStart', 'center', 'centerEnd',
//   'bottomStart', 'bottomCenter', 'bottomEnd')
@Serializable
enum class ScaleContentAlignment {
    TOP_START, // 0
    TOP_CENTER, // 1
    TOP_END, // 2
    CENTER_START, // 3
    CENTER, // 4
    CENTER_END, // 5
    BOTTOM_START, // 6
    BOTTOM_CENTER, // 7
    BOTTOM_END, // 8
}

// HorizontalAlignment = Status('start', 'center', 'end')
@Serializable
enum class ScaleHorizontalAlignment {
    START, // 0
    CENTER, // 1
    END, // 2
}

// VerticalAlignment = Status('top', 'center', 'bottom')
@Serializable
enum class ScaleVerticalAlignment {
    TOP, // 0
    CENTER, // 1
    BOTTOM, // 2
}

// Arrangement = Status('start', 'end', 'center', 'spaceBetween', 'spaceAround', 'spaceEvenly')
@Serializable
enum class ScaleArrangement {
    START, // 0
    END, // 1
    CENTER, // 2
    SPACE_BETWEEN, // 3
    SPACE_AROUND, // 4
    SPACE_EVENLY, // 5
}
