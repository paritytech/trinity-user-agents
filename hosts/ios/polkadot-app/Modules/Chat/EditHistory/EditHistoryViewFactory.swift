import CoreData
import Foundation
import PolkadotUI
import UIKit

@MainActor
enum EditHistoryViewFactory {
    static func createView(messageId: String) -> EditHistoryViewController {
        let sortDescriptor = NSSortDescriptor(
            key: #keyPath(CDChatMessage.timestamp),
            ascending: false
        )
        let repositoryFactory = ChatMessageRepositoryFactory()
        let repository = repositoryFactory.createRepository(
            forFilter: .editHistory(for: messageId),
            sortDescriptors: [sortDescriptor]
        )

        let interactor = EditHistoryInteractor(messageId: messageId, repository: repository)
        let view = createView(messageId: messageId, interactor: interactor)
        interactor.presenter = view.presenter as? EditHistoryPresenter
        return view
    }

    /// The edit history of a message of a room that the product keeps.
    static func createView(messageId: String, interactor: ProductEditHistoryInteractor) -> EditHistoryViewController {
        let view = createView(messageId: messageId, interactor: interactor as EditHistoryInteractorInputProtocol)
        interactor.presenter = view.presenter as? EditHistoryPresenter
        return view
    }

    private static func createView(
        messageId: String,
        interactor: EditHistoryInteractorInputProtocol
    ) -> EditHistoryViewController {
        let timestampFormatter = ContactTimestampFormatter()
        let presenter = EditHistoryPresenter(
            messageId: messageId,
            timestampFormatter: timestampFormatter,
            interactor: interactor
        )
        let viewController = EditHistoryViewController(timestampFormatter: timestampFormatter)

        viewController.presenter = presenter
        presenter.view = viewController

        configureSheetPresentation(for: viewController)

        return viewController
    }
}

// MARK: - EditHistoryViewFactory

extension EditHistoryViewFactory {
    private static func configureSheetPresentation(for viewController: UIViewController) {
        viewController.modalPresentationStyle = .pageSheet

        guard let sheet = viewController.sheetPresentationController else {
            return
        }
        sheet.detents = [.custom { context in context.maximumDetentValue / 3 }, .large()]
        sheet.prefersGrabberVisible = true
        sheet.prefersScrollingExpandsWhenScrolledToEdge = true
        sheet.preferredCornerRadius = 32
    }
}
