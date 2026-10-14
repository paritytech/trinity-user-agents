import UIKit
public import UIKit_iOS

public class AutosizedSheetPresentationFactory: NSObject {
    let configuration: ModalSheetPresentationConfiguration

    weak var presentation: AutosizedSheetPresentationController?

    public init(configuration: ModalSheetPresentationConfiguration) {
        self.configuration = configuration

        super.init()
    }
}

extension AutosizedSheetPresentationFactory: UIViewControllerTransitioningDelegate {
    public func animationController(
        forPresented _: UIViewController,
        presenting _: UIViewController,
        source _: UIViewController
    ) -> UIViewControllerAnimatedTransitioning? {
        ModalSheetPresentationAppearanceAnimator(animator: configuration.contentAppearanceAnimator)
    }

    public func animationController(forDismissed _: UIViewController)
        -> UIViewControllerAnimatedTransitioning? {
        ModalSheetPresentationDismissAnimator(
            animator: configuration.contentDissmisalAnimator,
            finalPositionOffset: configuration.style.headerStyle?
                .preferredHeight ?? 0.0
        )
    }

    public func presentationController(
        forPresented presented: UIViewController,
        presenting: UIViewController?,
        source _: UIViewController
    ) -> UIPresentationController? {
        let presentation = AutosizedSheetPresentationController(
            presentedViewController: presented,
            presenting: presenting,
            configuration: configuration
        )

        self.presentation = presentation

        return presentation
    }

    public func interactionControllerForDismissal(using _: UIViewControllerAnimatedTransitioning)
        -> UIViewControllerInteractiveTransitioning? {
        presentation?.interactiveDismissal
    }
}
