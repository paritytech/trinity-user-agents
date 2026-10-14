import CoreData
import Foundation
import PolkadotUI
import UIKit

@MainActor
enum EditHistoryViewFactory {
    static func createView(messageId: String) -> EditHistoryViewController {
        let timestampFormatter = ContactTimestampFormatter()

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
        let presenter = EditHistoryPresenter(
            messageId: messageId,
            timestampFormatter: timestampFormatter,
            interactor: interactor
        )
        let viewController = EditHistoryViewController(timestampFormatter: timestampFormatter)

        viewController.presenter = presenter
        presenter.view = viewController
        interactor.presenter = presenter

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
