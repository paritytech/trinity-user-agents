package io.paritytech.polkadotapp.feature_products_impl.domain.bot.mock

import uniffi.truapi.Arrangement
import uniffi.truapi.Background
import uniffi.truapi.BlendingMode
import uniffi.truapi.BorderStyle
import uniffi.truapi.BoxProps
import uniffi.truapi.ButtonProps
import uniffi.truapi.ButtonVariant
import uniffi.truapi.ColorToken
import uniffi.truapi.ColumnProps
import uniffi.truapi.ContentAlignment
import uniffi.truapi.Dimensions
import uniffi.truapi.Effect
import uniffi.truapi.EffectProps
import uniffi.truapi.HorizontalAlignment
import uniffi.truapi.ImageFit
import uniffi.truapi.ImageProps
import uniffi.truapi.ImageSource
import uniffi.truapi.Modifier
import uniffi.truapi.RendererNode
import uniffi.truapi.RowProps
import uniffi.truapi.Shape
import uniffi.truapi.TextFieldProps
import uniffi.truapi.TextProps
import uniffi.truapi.TypographyStyle
import uniffi.truapi.VerticalAlignment

/**
 * The trees that the mock product draws through the renderer: the custom bodies of its messages and the custom
 * footer. Together they use every node, modifier, typography style and colour token.
 */
object MockChatRenderTrees {
    object Action {
        const val INCREMENT = "increment"
        const val RESET = "reset"
        const val PRIMARY = "primary"
        const val TYPED = "typed"
        const val FOOTER_YES = "footer-yes"
        const val FOOTER_NO = "footer-no"
        const val FOOTER_INPUT = "footer-input"
    }

    fun card(typed: String, primaryPresses: Int): RendererNode = column(
        listOf(Modifier.Padding(inset(16u)), Modifier.Background(Background(ColorToken.BG_SURFACE_CONTAINER, Shape.Rounded(16u))), Modifier.FillWidth(true)),
        listOf(
            text("Custom body", TypographyStyle.HEADLINE_LARGE),
            text("Drawn by the product through the renderer", TypographyStyle.TITLE_MEDIUM_REGULAR, ColorToken.FG_SECONDARY),
            colourSwatches(),
            shapes(),
            RendererNode.Spacer(listOf(Modifier.Height(8u))),
            RendererNode.Image(
                listOf(Modifier.Height(120u), Modifier.FillWidth(true), Modifier.Border(BorderStyle(1u, ColorToken.FG_TERTIARY, Shape.Rounded(12u)))),
                ImageProps(ImageSource.Bulletin("showcase-picture"), ImageFit.COVER),
            ),
            RendererNode.Effect(EffectProps(Effect.RAINBOW), listOf(text("An effect: rainbow", TypographyStyle.BODY_LARGE_REGULAR))),
            buttons(primaryPresses),
            RendererNode.TextField(
                listOf(Modifier.FillWidth(true)),
                TextFieldProps(typed, placeholder = "Type here", label = "A text field", enabled = true, valueChangeAction = Action.TYPED),
            ),
            text(if (typed.isEmpty()) "The product reads the field as you type." else "You typed: $typed", TypographyStyle.BODY_SMALL_REGULAR, ColorToken.FG_TERTIARY),
        ),
    )

    fun counter(count: Int): RendererNode = RendererNode.Row(
        listOf(Modifier.Padding(inset(12u)), Modifier.Border(BorderStyle(1u, ColorToken.FG_TERTIARY, Shape.Rounded(12u)))),
        RowProps(VerticalAlignment.CENTER, Arrangement.SPACE_BETWEEN),
        listOf(
            text("Pressed $count times", TypographyStyle.BODY_LARGE_REGULAR, if (count > 0) ColorToken.FG_SUCCESS else ColorToken.FG_PRIMARY),
            RendererNode.Row(
                emptyList(),
                RowProps(VerticalAlignment.CENTER, Arrangement.END),
                listOf(button("Reset", ButtonVariant.TEXT, Action.RESET, enabled = count > 0), button("+1", ButtonVariant.PRIMARY, Action.INCREMENT)),
            ),
        ),
    )

    fun footer(): RendererNode = column(
        listOf(Modifier.Padding(inset(12u)), Modifier.FillWidth(true)),
        listOf(
            text("A footer drawn by the product", TypographyStyle.BODY_MEDIUM_REGULAR, ColorToken.FG_SECONDARY),
            RendererNode.Row(
                listOf(Modifier.FillWidth(true)),
                RowProps(VerticalAlignment.CENTER, Arrangement.SPACE_EVENLY),
                listOf(
                    button("👍 Yes", ButtonVariant.PRIMARY, Action.FOOTER_YES),
                    button("👎 No", ButtonVariant.SECONDARY, Action.FOOTER_NO),
                    button("Input", ButtonVariant.TEXT, Action.FOOTER_INPUT),
                ),
            ),
        ),
    )

    private fun inset(size: ULong) = Dimensions(size, size, size, size)

    private fun text(value: String, style: TypographyStyle, color: ColorToken = ColorToken.FG_PRIMARY): RendererNode =
        RendererNode.Text(emptyList(), TextProps(style, color), listOf(RendererNode.String(value)))

    private fun column(modifiers: List<Modifier>, children: List<RendererNode>): RendererNode =
        RendererNode.Column(modifiers, ColumnProps(HorizontalAlignment.START, Arrangement.START), children)

    private fun button(title: String, variant: ButtonVariant, action: String, enabled: Boolean = true): RendererNode = RendererNode.Button(
        listOf(Modifier.Margin(Dimensions(0u, 4u, 0u, 4u))),
        ButtonProps(title, variant, enabled = enabled, loading = false, clickAction = action),
        emptyList(),
    )

    private fun colourSwatches(): RendererNode = column(
        listOf(Modifier.Margin(Dimensions(8u, 0u, 8u, 0u))),
        listOf(
            ColorToken.FG_PRIMARY to TypographyStyle.BODY_LARGE_REGULAR,
            ColorToken.FG_SECONDARY to TypographyStyle.BODY_MEDIUM_REGULAR,
            ColorToken.FG_TERTIARY to TypographyStyle.BODY_SMALL_REGULAR,
            ColorToken.FG_SUCCESS to TypographyStyle.BODY_MEDIUM_REGULAR,
            ColorToken.FG_WARNING to TypographyStyle.BODY_MEDIUM_REGULAR,
            ColorToken.FG_ERROR to TypographyStyle.BODY_MEDIUM_REGULAR,
        ).map { (color, style) -> text(color.name, style, color) },
    )

    // The three backgrounds and the three shapes.
    private fun shapes(): RendererNode = RendererNode.Row(
        listOf(Modifier.FillWidth(true), Modifier.MinHeight(48u)),
        RowProps(VerticalAlignment.CENTER, Arrangement.SPACE_AROUND),
        listOf(
            Background(ColorToken.BG_SURFACE_MAIN, Shape.Rounded(8u)) to BlendingMode.NORMAL,
            Background(ColorToken.BG_SURFACE_NESTED, Shape.Circle) to BlendingMode.MULTIPLY,
            Background(ColorToken.FG_SUCCESS, Shape.Square) to BlendingMode.SCREEN,
        ).map { (background, blending) ->
            RendererNode.Box(
                listOf(Modifier.Width(48u), Modifier.Height(48u), Modifier.MinWidth(48u), Modifier.Background(background), Modifier.BlendingMode(blending), Modifier.Opacity(220u)),
                BoxProps(ContentAlignment.CENTER),
                listOf(RendererNode.String("•")),
            )
        },
    )

    private fun buttons(primaryPresses: Int): RendererNode = RendererNode.Row(
        listOf(Modifier.FillWidth(true), Modifier.FillHeight(false)),
        RowProps(VerticalAlignment.CENTER, Arrangement.SPACE_BETWEEN),
        listOf(
            button(if (primaryPresses == 0) "Primary" else "Primary ×$primaryPresses", ButtonVariant.PRIMARY, Action.PRIMARY),
            button("Secondary", ButtonVariant.SECONDARY, Action.PRIMARY),
            RendererNode.Button(emptyList(), ButtonProps("Loading", ButtonVariant.TEXT, enabled = false, loading = true, clickAction = null), emptyList()),
        ),
    )
}
