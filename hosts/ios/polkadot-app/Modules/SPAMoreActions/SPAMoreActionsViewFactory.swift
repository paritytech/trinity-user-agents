import UIKit

@MainActor
enum SPAMoreActionsViewFactory {
    static func createView(
        actions: [SPAMoreAction],
        closeTitle: String
    ) -> SPAMoreActionsViewProtocol {
        let presenter = SPAMoreActionsPresenter(
            actions: actions,
            closeTitle: closeTitle
        )
        let view = SPAMoreActionsViewController(presenter: presenter)
        presenter.view = view

        let rowHeight: CGFloat = 48
        let closeHeight: CGFloat = 44
        let separatorHeight: CGFloat = 8
        let topInset: CGFloat = 24
        let bottomInset: CGFloat = 16
        let contentHeight = topInset + (rowHeight * CGFloat(actions.count)) + separatorHeight + closeHeight +
            bottomInset

        BottomSheetViewFacade.setupBottomSheet(from: view, preferredHeight: contentHeight)

        return view
    }
}
