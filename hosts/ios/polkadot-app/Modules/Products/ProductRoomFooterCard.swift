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

    var safeAreaFillColor: UIColor? {
        UIColor.bgSurfaceMain
    }

    @MainActor
    func makeContentView(for _: ChatInputHandling?) -> UIView {
        UIHostingConfiguration {
            ProductRoomFooterCardContent(
                roomId: roomId,
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
}

/// What a docked card shows, as far as the hand-back after a slide goes.
///
/// A product that docks a slide to confirm in the footer keeps the same card while the slide runs:
/// its tree moves from the idle slider to the slid one and on to the one waiting on the product, and
/// the slider has to keep its place through all of them. When the product is done, the same card's
/// tree turns into an ordinary card, and that change of face is what the footer crossfades.
enum ProductRoomFooterFace: Hashable {
    /// No tree yet: the card is still being drawn.
    case drawing
    /// A tree holding a slide to confirm, in any of its states.
    case slide
    /// Any other tree, such as the card a slide hands the footer back to.
    case card

    /// How long a slide takes to hand the footer back to a card: the crossfade between two room footers.
    static let handBackDuration: TimeInterval = 0.25

    init(node: CustomMessageWidgetNode?) {
        guard let node else {
            self = .drawing
            return
        }
        self = Self.holdsSlide(node) ? .slide : .card
    }

    private static func holdsSlide(_ node: CustomMessageWidgetNode) -> Bool {
        switch node.content {
        case .slideToConfirm,
             .gesture:
            true
        case let .box(_, children),
             let .column(_, children),
             let .row(_, children):
            children.contains(where: holdsSlide)
        case .spacer,
             .text,
             .button,
             .textField,
             .image:
            false
        }
    }
}

/// The docked card's tree. A slide handing the footer back to a card crossfades into it, as one footer
/// does into the next; within a face the tree only updates, so a slid thumb keeps its place.
private struct ProductRoomFooterCardContent: View {
    let roomId: String
    let nodeProvider: any WidgetNodeProviding
    let onAction: WidgetActionHandler
    let resolveImage: WidgetImageResolver

    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        let face = ProductRoomFooterFace(node: nodeProvider.node)
        ZStack {
            ProductWidgetChatView(
                messageId: roomId,
                nodeProvider: nodeProvider,
                onAction: onAction,
                resolveImage: resolveImage
            )
            .id(face)
            .transition(.opacity)
        }
        .animation(reduceMotion ? nil : .easeInOut(duration: ProductRoomFooterFace.handBackDuration), value: face)
    }
}
