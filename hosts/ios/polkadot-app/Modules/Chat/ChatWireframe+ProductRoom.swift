import Foundation
import UIKit
import UIKitExt

/// The navigation of the parts of a product room that the product declares (P6).
@MainActor
protocol ChatProductRoomWireframing: AnyObject {
    /// Asks the user to confirm a link of a product action, then opens it (02-DESIGN §6.6, scenario S27).
    func confirmProductLink(_ url: URL, from view: ControllerBackedProtocol?, onOpen: @escaping () -> Void)

    /// Opens a link of a product action: a `polkadot:` link through the link pipeline of the app, an `https` link
    /// in the in-app browser, and any other link through the system.
    func openProductLink(_ url: URL, from view: ControllerBackedProtocol?)

    /// Shows the participant list of a product room (P6.3).
    func showParticipants(of productRoom: ChatProductRoomPresenter, from view: ControllerBackedProtocol?)

    /// Shows the sheet of a participant with its entries (P6.4).
    func showParticipantSheet(
        _ viewModel: ChatParticipantSheetViewModel,
        from view: ControllerBackedProtocol?,
        onSelect: @escaping (ChatParticipantSheetEntry) -> Void
    )

    /// Opens the App of a product.
    func openProduct(_ productId: String)

    /// Asks the user for the kind of attachment (P6.7).
    func chooseAttachmentKind(
        _ kinds: [ChatAttachmentKind],
        from view: ControllerBackedProtocol?,
        onSelect: @escaping (ChatAttachmentKind) -> Void
    )

    /// Lets the user pick one attachment of a kind, and gives its bytes.
    func pickAttachment(
        _ kind: ChatAttachmentKind,
        from view: ControllerBackedProtocol?,
        completion: @escaping (PickedAttachment) -> Void
    )
}

extension ChatWireframe: ChatProductRoomWireframing {
    func confirmProductLink(_ url: URL, from view: ControllerBackedProtocol?, onOpen: @escaping () -> Void) {
        let openAction = AlertPresentableAction(title: String(localized: .ChatExtension.linkOpenAction)) {
            onOpen()
        }

        let viewModel = AlertPresentableViewModel(
            title: String(localized: .ChatExtension.linkOpenTitle),
            message: url.absoluteString,
            actions: [openAction],
            closeActionTitle: String(localized: .Common.cancel)
        )

        present(viewModel: viewModel, style: .alert, from: view)
    }

    func showParticipants(of productRoom: ChatProductRoomPresenter, from view: ControllerBackedProtocol?) {
        let participantsView = ChatParticipantsViewFactory.createView(
            productRoom: productRoom,
            iconViewModelFactory: flowState.flowState.iconViewModelFactory
        )
        view?.controller.navigationController?.pushViewController(participantsView.controller, animated: true)
    }

    func showParticipantSheet(
        _ viewModel: ChatParticipantSheetViewModel,
        from view: ControllerBackedProtocol?,
        onSelect: @escaping (ChatParticipantSheetEntry) -> Void
    ) {
        let sheet = UIAlertController(
            title: viewModel.title,
            message: viewModel.detail.isEmpty ? nil : viewModel.detail,
            preferredStyle: .actionSheet
        )

        for entry in viewModel.entries {
            sheet
                .addAction(UIAlertAction(
                    title: entry.title,
                    style: entry.isDestructive ? .destructive : .default
                ) { _ in
                    onSelect(entry)
                })
        }
        sheet.addAction(UIAlertAction(title: String(localized: .Common.cancel), style: .cancel))

        let presenter = view?.controller.presentedViewController ?? view?.controller
        presenter?.present(sheet, animated: true)
    }

    func openProduct(_ productId: String) {
        ProductOpener().open(productId: productId)
    }

    func chooseAttachmentKind(
        _ kinds: [ChatAttachmentKind],
        from view: ControllerBackedProtocol?,
        onSelect: @escaping (ChatAttachmentKind) -> Void
    ) {
        let sheet = UIAlertController(title: nil, message: nil, preferredStyle: .actionSheet)
        for kind in kinds {
            sheet.addAction(UIAlertAction(title: kind.title, style: .default) { _ in onSelect(kind) })
        }
        sheet.addAction(UIAlertAction(title: String(localized: .Common.cancel), style: .cancel))
        view?.controller.present(sheet, animated: true)
    }

    func pickAttachment(
        _ kind: ChatAttachmentKind,
        from view: ControllerBackedProtocol?,
        completion: @escaping (PickedAttachment) -> Void
    ) {
        guard let controller = view?.controller else { return }
        productAttachmentPicker.present(kind, from: controller, completion: completion)
    }

    func openProductLink(_ url: URL, from view: ControllerBackedProtocol?) {
        ChatProductLinkOpener(linkHandler: DeferredLinkHandler.shared).open(url) { [weak self, weak view] webURL in
            guard let view else { return }
            self?.showWeb(url: webURL, from: view, style: WebPresentableStyle(mode: .modal(.fullScreen)))
        }
    }
}

/// Picks the way to open a link of a product action.
struct ChatProductLinkOpener {
    enum Route: Equatable {
        /// The link pipeline of the app takes it: a product page, a room, a Pocket card.
        case app
        case web
        case system
    }

    let linkHandler: DeferredLinkHandling

    static func route(for url: URL) -> Route {
        switch url.scheme?.lowercased() {
        case "polkadot": .app
        case "http",
             "https": .web
        default: .system
        }
    }

    @MainActor
    func open(_ url: URL, showWeb: (URL) -> Void) {
        switch Self.route(for: url) {
        case .app:
            linkHandler.handle(with: url)
        case .web:
            showWeb(url)
        case .system:
            UIApplication.shared.open(url)
        }
    }
}

private extension ChatAttachmentKind {
    var title: String {
        switch self {
        case .photo: String(localized: .ChatExtension.attachmentPhoto)
        case .video: String(localized: .ChatExtension.attachmentVideo)
        case .file: String(localized: .ChatExtension.attachmentFile)
        }
    }
}

private extension ChatParticipantSheetEntry {
    var title: String {
        switch self {
        case .copyAddress: String(localized: .ChatExtension.participantCopyAddress)
        case .openProduct: String(localized: .ChatExtension.participantOpenProduct)
        case let .product(action): action.label
        }
    }

    var isDestructive: Bool {
        if case let .product(action) = self { action.destructive } else { false }
    }
}
