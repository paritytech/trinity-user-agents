import Foundation
import ZipArchive

protocol LogsEmailDraftMaking {
    func makeLogsDraft() -> EmailDraft?
}

final class LogsEmailDraftFactory: LogsEmailDraftMaking {
    private let archiveURL: URL?

    private lazy var subjectDateFormatter: DateFormatter = {
        let dateFormatter = DateFormatter()
        dateFormatter.dateFormat = "dd/MM/yyyy"
        return dateFormatter
    }()

    init(archiveURL: URL? = nil) {
        self.archiveURL = archiveURL
    }

    func makeLogsDraft() -> EmailDraft? {
        guard
            let logsURL = fileManager.logsDirectoryURL(),
            let zipURL = archiveURL ?? documentDirectoryURL()?.appendingPathComponent("Logs.zip")
        else {
            return nil
        }

        let zipName = zipURL.lastPathComponent

        if fileManager.fileExists(atPath: zipURL.path) {
            try? fileManager.removeItem(at: zipURL)
        }

        let success = SSZipArchive.createZipFile(
            atPath: zipURL.path,
            withContentsOfDirectory: logsURL.path
        )

        guard success, let data = try? Data(contentsOf: zipURL) else {
            return nil
        }

        return EmailDraft(
            subject: "\(subjectDateFormatter.string(from: Date())) - iOS",
            message: "\n\n\n",
            // No recipient: the log archive is a debug aid, and who receives it is the
            // user's call — the mail composer opens with an empty To field for them to fill.
            recipients: [],
            attachment: .init(
                data: data,
                mimeType: "application/octet-stream",
                name: zipName,
                url: zipURL
            )
        )
    }
}

private extension LogsEmailDraftFactory {
    var fileManager: FileManager {
        .default
    }

    func documentDirectoryURL() -> URL? {
        let urls = fileManager.urls(for: .documentDirectory, in: .userDomainMask)
        return urls.last ?? nil
    }
}
