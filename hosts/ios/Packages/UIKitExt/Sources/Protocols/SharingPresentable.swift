import UIKit

public typealias SharingCompletionHandler = (Bool) -> Void

@MainActor
public protocol SharingPresentable {
    func share(
        source: UIActivityItemSource,
        from view: ControllerBackedProtocol?,
        applicationActivities: [UIActivity]?,
        excludedActivityTypes: [UIActivity.ActivityType]?,
        with completionHandler: SharingCompletionHandler?
    )
    func share(
        items: [Any],
        from view: ControllerBackedProtocol?,
        applicationActivities: [UIActivity]?,
        excludedActivityTypes: [UIActivity.ActivityType]?,
        with completionHandler: SharingCompletionHandler?
    )
}

public extension SharingPresentable {
    func share(
        source: UIActivityItemSource,
        from view: ControllerBackedProtocol?,
        applicationActivities: [UIActivity]? = nil,
        excludedActivityTypes: [UIActivity.ActivityType]? = nil,
        with completionHandler: SharingCompletionHandler? = nil
    ) {
        share(
            items: [source],
            from: view,
            applicationActivities: applicationActivities,
            excludedActivityTypes: excludedActivityTypes,
            with: completionHandler
        )
    }

    func share(
        items: [Any],
        from view: ControllerBackedProtocol?,
        applicationActivities: [UIActivity]? = nil,
        excludedActivityTypes: [UIActivity.ActivityType]? = nil,
        with completionHandler: SharingCompletionHandler? = nil
    ) {
        let currentController = view?.controller ?? UIWindow.topWindow?.rootViewController
        guard let controller = currentController else { return }

        let activityViewController = UIActivityViewController(
            activityItems: items,
            applicationActivities: applicationActivities
        )
        activityViewController.excludedActivityTypes = excludedActivityTypes
        activityViewController.overrideUserInterfaceStyle = UITraitCollection.current.userInterfaceStyle
        activityViewController.popoverPresentationController?.sourceView = controller.view

        if let handler = completionHandler {
            activityViewController.completionWithItemsHandler = { _, completed, _, _ in
                handler(completed)
            }
        }

        controller.present(activityViewController, animated: true)
    }
}
