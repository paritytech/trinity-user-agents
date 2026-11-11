import UIKit

/// Holds the status strip and, on iOS 26, draws the top scroll edge effect of the bound scroll view under it.
final class StatusStripContainerView: UIView {
    private var scrollEdgeInteraction: UIInteraction?

    override init(frame: CGRect) {
        super.init(frame: frame)

        if #available(iOS 26.0, *) {
            let interaction = UIScrollEdgeElementContainerInteraction()
            interaction.edge = .top
            addInteraction(interaction)
            scrollEdgeInteraction = interaction
        }
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func bindScrollEdge(toContentOf controller: UIViewController?) {
        guard
            #available(iOS 26.0, *),
            let interaction = scrollEdgeInteraction as? UIScrollEdgeElementContainerInteraction
        else {
            return
        }

        let content = (controller as? UINavigationController)?.topViewController ?? controller
        interaction.scrollView = content?.contentScrollView(for: .top)
    }
}
