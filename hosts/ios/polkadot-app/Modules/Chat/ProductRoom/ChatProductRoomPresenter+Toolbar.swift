import Foundation
import TrUAPIHost
import UIKit

/// The toolbar of a product room (P6.2, 02-DESIGN §6.6, scenario S29).
enum ChatToolbarLayout {
    /// The number of buttons that the bar shows. The other buttons go in the overflow menu.
    static let inlineCount = 2

    /// The buttons that the bar shows and the buttons of the overflow menu, in the order of the product.
    static func split<Item>(_ items: [Item]) -> (inline: [Item], overflow: [Item]) {
        (Array(items.prefix(inlineCount)), Array(items.dropFirst(inlineCount)))
    }
}

extension ChatProductRoomPresenter {
    /// Draws the toolbar at once with the labels, then again with the icons when they load.
    func provideToolbar() {
        let actions = policy?.toolbarActions ?? []
        view?.didReceive(productToolbar: toolbarItems(actions, icons: [:]))

        let (inline, overflow) = ChatToolbarLayout.split(actions.map(\.actionId))
        mark(
            "toolbar",
            "inline=\(inline.joined(separator: ",")) overflow=\(overflow.joined(separator: ","))"
                + " participants=\(asksParticipants)"
        )

        guard !actions.isEmpty else {
            return
        }

        Task { [weak self] in
            guard let self else { return }
            let icons = await icons(of: actions.map(\.menuAction))
            guard policy?.toolbarActions == actions else { return }
            view?.didReceive(productToolbar: toolbarItems(actions, icons: icons))
        }
    }

    /// Runs a toolbar button: a link opens after the user confirms it, a callback sends `ToolbarActionTriggered`.
    func pressToolbar(_ action: Chat.RoomToolbarAction) {
        perform(action.handler, event: .toolbarActionTriggered(actionId: action.actionId))
    }

    /// The bar items, the trailing item first: the inline buttons, then one overflow button with a menu. The
    /// participants button of the host goes after the buttons of the product.
    func toolbarItems(_ actions: [Chat.RoomToolbarAction], icons: [String: UIImage]) -> [UIBarButtonItem] {
        let (inline, overflow) = ChatToolbarLayout.split(actions)

        var items: [UIBarButtonItem] = participantsButton().map { [$0] } ?? []
        if !overflow.isEmpty {
            let entries = overflow.map { action in
                UIAction(title: action.label, image: icons[action.actionId].map(Self.barIcon)) { [weak self] _ in
                    self?.pressToolbar(action)
                }
            }
            let overflowItem = UIBarButtonItem(
                image: UIImage(systemName: "ellipsis.circle"),
                menu: UIMenu(children: entries)
            )
            overflowItem.accessibilityLabel = String(localized: .ChatExtension.toolbarMore)
            items.append(overflowItem)
        }

        // The bar draws the first item of the list at the trailing edge, so the first action goes last.
        items += inline.reversed().map { action in
            let primary = UIAction(
                title: action.label,
                image: icons[action.actionId].map(Self.barIcon)
            ) { [weak self] _ in
                self?.pressToolbar(action)
            }
            let item = UIBarButtonItem(primaryAction: primary)
            item.tintColor = .fgPrimary
            item.accessibilityLabel = action.label
            return item
        }

        return items
    }
}

extension ChatProductRoomPresenter {
    /// The side of a toolbar icon, in points.
    static let barIconSide: CGFloat = 24

    /// A product icon at the size of a bar button, in its own colours: the bar does not tint it.
    static func barIcon(_ image: UIImage) -> UIImage {
        let side = barIconSide
        let scale = min(side / max(image.size.width, 1), side / max(image.size.height, 1))
        let size = CGSize(width: image.size.width * scale, height: image.size.height * scale)
        return UIGraphicsImageRenderer(size: size).image { _ in
            image.draw(in: CGRect(origin: .zero, size: size))
        }
        .withRenderingMode(.alwaysOriginal)
    }
}

extension Chat.RoomToolbarAction {
    /// The button as a menu entry, for the loading of its icon.
    var menuAction: Chat.ProductMenuAction {
        Chat.ProductMenuAction(actionId: actionId, label: label, icon: icon, destructive: false, handler: handler)
    }
}
