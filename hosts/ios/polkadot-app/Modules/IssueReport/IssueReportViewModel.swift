import Foundation
import Keystore_iOS
import Observation
import Operation_iOS
import StructuredConcurrency

@MainActor
@Observable
final class IssueReportViewModel {
    static let descriptionLimit = 2_000

    var description = ""
    var gitHubUsername: String
    private(set) var isSending = false
    private(set) var errorMessage: String?
    var isSent = false

    private let screenshot: Data
    private let makeLogsDraft: (URL) -> EmailDraft?
    private let sender: any IssueReportSending
    private let settingsManager: SettingsManagerProtocol
    @ObservationIgnored private var sendTask: Task<Void, Never>?

    init(
        screenshot: Data,
        makeLogsDraft: @escaping (URL) -> EmailDraft?,
        sender: any IssueReportSending,
        settingsManager: SettingsManagerProtocol
    ) {
        self.screenshot = screenshot
        self.makeLogsDraft = makeLogsDraft
        self.sender = sender
        self.settingsManager = settingsManager
        gitHubUsername = settingsManager.string(for: .issueReportGitHubUsername) ?? ""
    }

    var isGitHubUsernameValid: Bool {
        gitHubUsername.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || GitHubUsername(gitHubUsername) != nil
    }

    var canSend: Bool {
        !isSending && !isSent && description.count <= Self.descriptionLimit &&
            !description.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && isGitHubUsernameValid
    }

    func send() {
        guard canSend else { return }
        let description = description.trimmingCharacters(in: .whitespacesAndNewlines)
        let username = GitHubUsername(gitHubUsername)
        if let username {
            settingsManager.set(string: username.value, for: .issueReportGitHubUsername)
        } else {
            settingsManager.removeValue(for: .issueReportGitHubUsername)
        }
        errorMessage = nil
        isSending = true
        sendTask = Task {
            defer {
                isSending = false
                sendTask = nil
            }

            do {
                let logs = try await ClosureOperation { [makeLogsDraft] in
                    let archiveURL = FileManager.default.temporaryDirectory
                        .appendingPathComponent("issue-logs-\(UUID().uuidString).zip")
                    defer { try? FileManager.default.removeItem(at: archiveURL) }
                    return makeLogsDraft(archiveURL)?.attachment?.data
                }.asyncExecute()
                try Task.checkCancellation()
                guard let logs, !logs.isEmpty else {
                    errorMessage = String(localized: .reportIssueCollectFailed)
                    return
                }
                try await sender.send(IssueReport(
                    description: description,
                    logs: logs,
                    screenshot: screenshot,
                    gitHubUsername: username
                ))
                try Task.checkCancellation()
                isSent = true
            } catch is CancellationError {
            } catch IssueReportSubmissionError.unavailable {
                errorMessage = String(localized: .reportIssueUnavailable)
            } catch IssueReportSubmissionError.tooLarge {
                errorMessage = String(localized: .reportIssueTooLarge)
            } catch {
                if !Task.isCancelled {
                    errorMessage = String(localized: .reportIssueSendFailed)
                }
            }
        }
    }

    func cancel() {
        sendTask?.cancel()
    }
}
