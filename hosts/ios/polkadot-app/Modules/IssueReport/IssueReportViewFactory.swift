import DesignSystem
import SwiftUI
import UIKit

@MainActor
enum IssueReportViewFactory {
    static func createView(screenshot: UIImage, data: Data) -> UIViewController {
        let viewModel = IssueReportViewModel(
            screenshot: data,
            makeLogsDraft: { LogsEmailDraftFactory(archiveURL: $0).makeLogsDraft() },
            sender: IssueProxyReportSender(configuration: {
                try await FirebaseFacade.shared.asyncWaitIssueProxyConfiguration()
            })
        )
        let controller = UIHostingController(rootView: IssueReportViewLayout(
            viewModel: viewModel,
            screenshot: screenshot
        ))
        controller.view.backgroundColor = .bgSurfaceMain
        controller.modalPresentationStyle = .pageSheet
        controller.sheetPresentationController?.detents = [.large()]
        controller.sheetPresentationController?.preferredCornerRadius = 32
        return controller
    }
}
