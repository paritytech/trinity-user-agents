#if TESTNET_FEATURE
    import Foundation
    import TrUAPIHost

    /// The trees that the mock product draws through the renderer (`Renderer::render`): the custom bodies of its
    /// messages and the custom footer. Together they use every node, modifier, typography style and colour token.
    enum MockChatRenderTrees {
        enum Action {
            static let increment = "increment"
            static let reset = "reset"
            static let primary = "primary"
            static let typed = "typed"
            static let footerYes = "footer-yes"
            static let footerNo = "footer-no"
            static let footerInput = "footer-input"
        }

        /// The card: what the renderer can draw, in one body. `typed` is the text of its field.
        static func card(typed: String, primaryPresses: Int) -> RendererNode {
            column(
                [
                    .padding(inset(16)),
                    .background(Background(color: .bgSurfaceContainer, shape: .rounded(16))),
                    .fillWidth(true)
                ],
                children: [
                    text("Custom body", style: .headlineLarge),
                    text("Drawn by the product through the renderer", style: .titleMediumRegular, color: .fgSecondary),
                    colourSwatches(),
                    shapes(),
                    .spacer(modifiers: [.height(8)]),
                    .image(
                        modifiers: [
                            .height(120),
                            .fillWidth(true),
                            .border(BorderStyle(width: 1, color: .fgTertiary, shape: .rounded(12)))
                        ],
                        props: ImageProps(source: .bulletin("showcase-picture"), fit: .cover)
                    ),
                    .effect(
                        props: EffectProps(effect: .rainbow),
                        children: [text("An effect: rainbow", style: .bodyLargeRegular)]
                    ),
                    buttons(primaryPresses: primaryPresses),
                    .textField(modifiers: [.fillWidth(true)], props: TextFieldProps(
                        text: typed,
                        placeholder: "Type here",
                        label: "A text field",
                        enabled: true,
                        valueChangeAction: Action.typed
                    )),
                    text(
                        typed.isEmpty ? "The product reads the field as you type." : "You typed: \(typed)",
                        style: .bodySmallRegular,
                        color: .fgTertiary
                    )
                ]
            )
        }

        /// A body that the product draws again after each press.
        static func counter(_ count: Int) -> RendererNode {
            .row(
                modifiers: [
                    .padding(inset(12)),
                    .border(BorderStyle(width: 1, color: .fgTertiary, shape: .rounded(12)))
                ],
                props: RowProps(verticalAlignment: .center, horizontalArrangement: .spaceBetween),
                children: [
                    text(
                        "Pressed \(count) times",
                        style: .bodyLargeRegular,
                        color: count > 0 ? .fgSuccess : .fgPrimary
                    ),
                    .row(
                        modifiers: [],
                        props: RowProps(verticalAlignment: .center, horizontalArrangement: .end),
                        children: [
                            button("Reset", variant: .text, action: Action.reset, enabled: count > 0),
                            button("+1", variant: .primary, action: Action.increment)
                        ]
                    )
                ]
            )
        }

        /// The custom footer (`ChatRoomFooter::Custom`).
        static func footer() -> RendererNode {
            column([.padding(inset(12)), .fillWidth(true)], children: [
                text("A footer drawn by the product", style: .bodyMediumRegular, color: .fgSecondary),
                .row(
                    modifiers: [.fillWidth(true)],
                    props: RowProps(verticalAlignment: .center, horizontalArrangement: .spaceEvenly),
                    children: [
                        button("👍 Yes", variant: .primary, action: Action.footerYes),
                        button("👎 No", variant: .secondary, action: Action.footerNo),
                        button("Input", variant: .text, action: Action.footerInput)
                    ]
                )
            ])
        }
    }

    private extension MockChatRenderTrees {
        static func inset(_ size: Size) -> Dimensions {
            Dimensions(top: size, end: size, bottom: size, start: size)
        }

        static func text(_ value: String, style: TypographyStyle, color: ColorToken = .fgPrimary) -> RendererNode {
            .text(modifiers: [], props: TextProps(style: style, color: color), children: [.string(text: value)])
        }

        static func column(_ modifiers: [Modifier], children: [RendererNode]) -> RendererNode {
            .column(
                modifiers: modifiers,
                props: ColumnProps(horizontalAlignment: .start, verticalArrangement: .start),
                children: children
            )
        }

        static func button(
            _ title: String,
            variant: ButtonVariant,
            action: String,
            enabled: Bool = true
        ) -> RendererNode {
            .button(
                modifiers: [.margin(Dimensions(top: 0, end: 4, bottom: 0, start: 4))],
                props: ButtonProps(
                    text: title,
                    variant: variant,
                    enabled: enabled,
                    loading: false,
                    clickAction: action
                ),
                children: []
            )
        }

        /// One line for each colour token, in the body styles.
        static func colourSwatches() -> RendererNode {
            let tokens: [(ColorToken, TypographyStyle)] = [
                (.fgPrimary, .bodyLargeRegular),
                (.fgSecondary, .bodyMediumRegular),
                (.fgTertiary, .bodySmallRegular),
                (.fgSuccess, .bodyMediumRegular),
                (.fgWarning, .bodyMediumRegular),
                (.fgError, .bodyMediumRegular)
            ]
            return column([.margin(Dimensions(top: 8, end: 0, bottom: 8, start: 0))], children: tokens.map {
                text(String(describing: $0.0), style: $0.1, color: $0.0)
            })
        }

        /// The three backgrounds and the three shapes, in a box, a row and a column.
        static func shapes() -> RendererNode {
            let swatches = [
                (Background(color: .bgSurfaceMain, shape: .rounded(8)), BlendingMode.normal),
                (Background(color: .bgSurfaceNested, shape: .circle), BlendingMode.multiply),
                (Background(color: .fgSuccess, shape: .square), BlendingMode.screen)
            ]
            return .row(
                modifiers: [.fillWidth(true), .minHeight(48)],
                props: RowProps(verticalAlignment: .center, horizontalArrangement: .spaceAround),
                children: swatches.map { background, blending in
                    .box(
                        modifiers: [
                            .width(48),
                            .height(48),
                            .minWidth(48),
                            .background(background),
                            .blendingMode(blending),
                            .opacity(220)
                        ],
                        props: BoxProps(contentAlignment: .center),
                        children: [.string(text: "•")]
                    )
                }
            )
        }

        static func buttons(primaryPresses: Int) -> RendererNode {
            .row(
                modifiers: [.fillWidth(true), .fillHeight(false)],
                props: RowProps(verticalAlignment: .center, horizontalArrangement: .spaceBetween),
                children: [
                    button(
                        primaryPresses == 0 ? "Primary" : "Primary ×\(primaryPresses)",
                        variant: .primary,
                        action: Action.primary
                    ),
                    button("Secondary", variant: .secondary, action: Action.primary),
                    .button(
                        modifiers: [],
                        props: ButtonProps(
                            text: "Loading",
                            variant: .text,
                            enabled: false,
                            loading: true,
                            clickAction: nil
                        ),
                        children: []
                    )
                ]
            )
        }
    }
#endif
