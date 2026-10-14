import UIKit
import FoundationExt
import UIKitExt

final class BottomNotificationViewController: UIViewController, ControllerBackedProtocol, ViewHolder {
    typealias RootViewType = BottomNotificationLayout

    let notificationTitle: String
    let dismissAfter: CGFloat
    let feedback: UINotificationFeedbackGenerator.FeedbackType

    let generator = UINotificationFeedbackGenerator()

    private var alreadyHandled: Bool = false

    init(
        notificationTitle: String,
        dismissAfter: CGFloat = 1.5,
        feedback: UINotificationFeedbackGenerator.FeedbackType = .success
    ) {
        self.notificationTitle = notificationTitle
        self.dismissAfter = dismissAfter
        self.feedback = feedback

        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func loadView() {
        view = BottomNotificationLayout()
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        rootView.isUserInteractionEnabled = false
        rootView.titleLabel.text = notificationTitle
    }

    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)

        if !alreadyHandled {
            alreadyHandled.toggle()
            notifyAndScheduleDismiss()
        }
    }

    override func viewWillDisappear(_: Bool) {
        super.viewWillDisappear(true)

        cancelDismissalTimer()
    }

    func notifyAndScheduleDismiss() {
        generator.notificationOccurred(feedback)

        perform(#selector(actionDidCancel), with: self, afterDelay: dismissAfter)
    }

    private func cancelDismissalTimer() {
        NSObject.cancelPreviousPerformRequests(
            withTarget: self,
            selector: #selector(actionDidCancel),
            object: nil
        )
    }

    @objc func actionDidCancel() {
        cancelDismissalTimer()

        presentingViewController?.dismiss(animated: true)
    }
}
