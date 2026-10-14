import Foundation

extension FileManager {
    func logsDirectoryURL() -> URL? {
        guard let url = try? url(
            for: .applicationSupportDirectory,
            in: .userDomainMask,
            appropriateFor: nil,
            create: true
        ) else {
            return nil
        }
        return url.appendingPathComponent("Logs", isDirectory: true)
    }

    func appLogsDirectoryURL() -> URL? {
        logsDirectoryURL()?
            .appendingPathComponent("App", isDirectory: true)
    }

    func newLogFileURL(inDirectory directoryURL: URL) -> URL {
        directoryURL
            .appendingPathComponent(makeLogFileName(), isDirectory: false)
    }

    func cleanupLogDirectory(at directoryURL: URL) {
        guard
            let logFiles = try? sortedContentsOfDirectory(at: directoryURL),
            logFiles.count > Self.previousLogSessionsToKeep
        else {
            return
        }

        let logsToRemove = logFiles.prefix(logFiles.count - Self.previousLogSessionsToKeep)
        logsToRemove.forEach { try? removeItem(at: $0) }
    }
}

private extension FileManager {
    static let previousLogSessionsToKeep = 10

    static var logFilesDateFormatter: DateFormatter = {
        let dateFormatter = DateFormatter()
        dateFormatter.locale = Locale(identifier: "en_US_POSIX")
        dateFormatter.dateFormat = "yyyy_MM_dd_HH_mm_ss"
        dateFormatter.timeZone = TimeZone(secondsFromGMT: 0)
        return dateFormatter
    }()

    func makeLogFileName() -> String {
        "\(Self.logFilesDateFormatter.string(from: Date())).log"
    }

    func sortedContentsOfDirectory(at url: URL) throws -> [URL] {
        let key = URLResourceKey.creationDateKey
        var files = try contentsOfDirectory(
            at: url,
            includingPropertiesForKeys: [key],
            options: .skipsHiddenFiles
        )

        try files.sort {
            let values1 = try $0.resourceValues(forKeys: [key])
            let values2 = try $1.resourceValues(forKeys: [key])

            if let date1 = values1.allValues.first?.value as? Date,
               let date2 = values2.allValues.first?.value as? Date {
                return date1.compare(date2) == .orderedAscending
            }

            return true
        }

        return files
    }
}
