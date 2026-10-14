import UIKit
import UIKitExt

enum ShareActivityResult {
    case success
    case cancelled
    case failure(Error)
}

/// Protocol that provides ability to share items using system component
@MainActor
protocol ShareActivityPresenting: AnyObject {
    /// Method dependency injection for presenter that will present share sheet
    /// - Parameter presenter: View Controller to present share sheet
    func use(presenter: ControllerBackedProtocol)
    /// Share list of `activityItems` using system component
    /// - Parameters:
    ///   - activityItems: list of items to share, .i.e. links, text, images
    ///   - completion: completion closure that indicates whether share was successful, cancelled or failed
    func share(activityItems: [Any], _ completion: @escaping (ShareActivityResult) -> Void)
}

final class ShareActivityAdapter: ShareActivityPresenting {
    enum ExcludedTypes {
        static let `default`: [UIActivity.ActivityType] = [
            .assignToContact,
            .saveToCameraRoll,
            .postToFacebook,
            .postToFlickr,
            .postToVimeo,
            .openInIBooks,
            .markupAsPDF
        ]
    }

    private weak var presenter: ControllerBackedProtocol!

    private let activityViewClass: UIActivityViewController.Type
    private let excludedTypes: [UIActivity.ActivityType]

    init(
        activityViewClass: UIActivityViewController.Type = UIActivityViewController.self,
        excludedTypes: [UIActivity.ActivityType] = ShareActivityAdapter.ExcludedTypes.default
    ) {
        self.activityViewClass = activityViewClass
        self.excludedTypes = excludedTypes
    }

    func use(presenter: ControllerBackedProtocol) {
        self.presenter = presenter
    }

    @MainActor
    func share(activityItems: [Any], _ completion: @escaping (ShareActivityResult) -> Void) {
        let shareController = activityViewClass.init(
            activityItems: activityItems,
            applicationActivities: []
        )

        shareController.excludedActivityTypes = excludedTypes
        shareController.completionWithItemsHandler = { _, isCompleted, _, error in
            if let error {
                completion(.failure(error))
                return
            }
            completion(isCompleted ? .success : .cancelled)
        }
        presenter.controller.present(shareController, animated: true)
    }
}
