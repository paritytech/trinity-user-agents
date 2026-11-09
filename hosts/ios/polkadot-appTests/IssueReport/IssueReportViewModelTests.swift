import Foundation
import Keystore_iOS
import Testing
@testable import polkadot_app

@MainActor
struct IssueReportViewModelTests {
    @Test func sentGitHubUsernamePrefillsTheNextReportAndAnEmptyFieldForgetsIt() {
        let settings = InMemorySettingsManager()

        send(gitHubUsername: " @octocat ", settings: settings)
        let prefilled = makeViewModel(settings: settings).gitHubUsername
        send(gitHubUsername: "", settings: settings)

        #expect([prefilled, makeViewModel(settings: settings).gitHubUsername] == ["octocat", ""])
    }

    @Test func invalidGitHubUsernameBlocksSending() {
        let viewModel = makeViewModel(settings: InMemorySettingsManager())
        viewModel.description = "The screen froze"
        viewModel.gitHubUsername = "octo cat"

        #expect(!viewModel.canSend)
    }
}

@MainActor
private extension IssueReportViewModelTests {
    struct AcceptingSender: IssueReportSending {
        func send(_: IssueReport) async throws {}
    }

    func makeViewModel(settings: SettingsManagerProtocol) -> IssueReportViewModel {
        IssueReportViewModel(
            screenshot: Data(),
            makeLogsDraft: { _ in nil },
            sender: AcceptingSender(),
            settingsManager: settings
        )
    }

    func send(gitHubUsername: String, settings: SettingsManagerProtocol) {
        let viewModel = makeViewModel(settings: settings)
        viewModel.description = "The screen froze"
        viewModel.gitHubUsername = gitHubUsername
        viewModel.send()
        viewModel.cancel()
    }
}
