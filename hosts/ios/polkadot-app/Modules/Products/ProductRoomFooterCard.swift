import DesignSystem
import Foundation
import Observation
import PolkadotUI
import Products
import SwiftUI
import UIKit

/// The input slot holding the card a product docked in the footer of its room.
struct ProductRoomFooterCardConfiguration: @preconcurrency ChatInputViewConfigurationProtocol {
    let roomId: String
    let messageType: String
    let payload: Data
    let nodeProvider: any WidgetNodeProviding
    let onAction: WidgetActionHandler
    let resolveImage: WidgetImageResolver

    var activateOnAppear: Bool {
        false
    }

    /// The strip below the slot, down to the screen edge, takes the chat's colour, so no chat
    /// shows under the app's game pill or the home indicator.
    var safeAreaFillColor: UIColor? {
        .bgSurfaceMain
    }

    /// A card that replaces the product's slider, or another card, fades in once it has drawn.
    var animatesReplacement: Bool {
        true
    }

    @MainActor
    func makeContentView(for _: ChatInputHandling?) -> UIView {
        UIHostingConfiguration {
            ProductWidgetChatView(
                messageId: roomId,
                nodeProvider: nodeProvider,
                onAction: onAction,
                resolveImage: resolveImage
            )
            .padding(.horizontal, 16)
            .padding(.top, DSSpacings.extraMedium)
            .padding(.bottom, DSSpacings.small)
            .background(
                LinearGradient(
                    colors: [Color.bgSurfaceMain.opacity(0), Color.bgSurfaceMain],
                    startPoint: .top,
                    endPoint: UnitPoint(x: 0.5, y: 0.3)
                )
            )
        }
        .margins(.all, 0)
        .makeContentView()
    }

    func equalsTo(configuration: any ChatInputViewConfigurationProtocol) -> Bool {
        guard let other = configuration as? ProductRoomFooterCardConfiguration else { return false }

        return roomId == other.roomId && messageType == other.messageType && payload == other.payload
    }

    /// Ready once the product has drawn the card, so the slot never shows the spinner in between.
    @MainActor
    func whenContentReady(_: UIView, _ ready: @escaping () -> Void) {
        Self.whenNodeArrives(nodeProvider, ready)
    }
}

@MainActor
private extension ProductRoomFooterCardConfiguration {
    static func whenNodeArrives(_ provider: any WidgetNodeProviding, _ ready: @escaping () -> Void) {
        guard provider.node == nil else {
            ready()
            return
        }
        _ = withObservationTracking {
            provider.node
        } onChange: { [weak provider] in
            Task { @MainActor in
                guard let provider else { return }
                whenNodeArrives(provider, ready)
            }
        }
    }
}
