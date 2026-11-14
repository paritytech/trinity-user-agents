import DesignSystem
import Foundation
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
            .padding(.vertical, 12)
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
}
