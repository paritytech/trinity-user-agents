import DesignSystem
import PolkadotUI
import SwiftUI
import Testing
import TrUAPIHost
@testable import polkadot_app

@MainActor
struct PocketCardThemeTests {
    private let fgPrimaryBoxHeight = 40

    private var faceUsingThemeDefaults: RendererNode {
        .column(
            modifiers: [.fillWidth(true), .fillHeight(true)],
            props: ColumnProps(horizontalAlignment: nil, verticalArrangement: nil),
            children: [
                .box(
                    modifiers: [
                        .fillWidth(true),
                        .height(UInt64(fgPrimaryBoxHeight)),
                        .background(Background(color: .fgPrimary, shape: nil))
                    ],
                    props: BoxProps(contentAlignment: nil),
                    children: []
                ),
                .text(modifiers: [], props: TextProps(style: nil, color: nil), children: [.string(text: "Loyalty")]),
                .button(
                    modifiers: [],
                    props: ButtonProps(text: "Remove", variant: .text, enabled: nil, loading: nil, clickAction: nil),
                    children: []
                ),
                .button(
                    modifiers: [],
                    props: ButtonProps(text: "Open", variant: .secondary, enabled: nil, loading: nil, clickAction: nil),
                    children: []
                ),
                .textField(
                    modifiers: [],
                    props: TextFieldProps(
                        text: "Ada",
                        placeholder: nil,
                        label: nil,
                        enabled: nil,
                        valueChangeAction: nil
                    )
                )
            ]
        )
    }

    @Test
    func aProductCardIsDrawnInBerlinNightWhateverThemeTheUserPicked() throws {
        let frame = PocketCardSize.testFrame
        let face = try #require(faceUsingThemeDefaults.toWidgetNode(resolver: WidgetDesignTokenResolver()))
        let card = PocketProductCardView(title: "Loyalty", face: face, resolveImage: nil)
        let inBerlinNight = try WidgetNodeRaster.pixels(of: card, in: frame, under: .berlinNight)

        let centreOfFgPrimaryBox = (fgPrimaryBoxHeight / 2 * Int(frame.width) + Int(frame.width) / 2) * 4
        let fgPrimary = Array(inBerlinNight[centreOfFgPrimaryBox ..< centreOfFgPrimaryBox + 3])
        #expect(fgPrimary == [0xF4, 0xF4, 0xF5], "the RFC's FgPrimary")

        for theme in ThemeSelection.allCases where theme != .berlinNight {
            let looksTheSame = try WidgetNodeRaster.pixels(of: card, in: frame, under: theme) == inBerlinNight
            #expect(looksTheSame, "\(theme)")
        }
    }
}
