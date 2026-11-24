import PolkadotUI
import SwiftUI
import Testing
import UIKit
@testable import polkadot_app

/// A product that replaces the card docked in its room's footer fades the new card in over the old one.
@MainActor
struct ProductRoomFooterCardTests {
    @MainActor
    @Observable
    final class Tree: WidgetNodeProviding {
        var node: CustomMessageWidgetNode?
    }

    private struct OtherSlot: ChatInputViewConfigurationProtocol {
        var activateOnAppear: Bool {
            false
        }

        func makeContentView(for _: ChatInputHandling?) -> UIView {
            UIView()
        }

        func equalsTo(configuration: any ChatInputViewConfigurationProtocol) -> Bool {
            configuration is OtherSlot
        }
    }

    private static func card(room: String = "room", type: String = "game", payload: [UInt8] = [1])
        -> ProductRoomFooterCardConfiguration {
        ProductRoomFooterCardConfiguration(
            roomId: room,
            messageType: type,
            payload: Data(payload),
            nodeProvider: Tree(),
            onAction: { _, _ in },
            resolveImage: WidgetImageResolver { _ in nil }
        )
    }

    @Test("Another card docked in the same room crossfades from the one before")
    func anotherCardInTheRoomCrossfades() {
        #expect(Self.card(payload: [2]).crossfades(from: Self.card(payload: [1])))
        #expect(Self.card(type: "result").crossfades(from: Self.card(type: "game")))
    }

    @Test("The same card, a card of another room and a slot that is no card are cut to")
    func otherReplacementsCut() {
        #expect(!Self.card().crossfades(from: Self.card()))
        #expect(!Self.card(room: "other").crossfades(from: Self.card(room: "room")))
        #expect(!Self.card().crossfades(from: OtherSlot()))
    }
}
