import Foundation

protocol AttachmentStoring: Sendable {
    @discardableResult
    func store(attachment: Data, filename: String) throws -> URL

    func loadAttachment(by filename: String) throws -> Data

    func createDirectoryIfNeeded() throws

    func fileURL(for filename: String) -> URL

    func hasFile(for filename: String) -> Bool

    func remove(for filename: String) throws

    @discardableResult
    func createEmptyFile(for filename: String) throws -> URL

    func moveFile(from sourceFilename: String, to destinationFilename: String) throws
}

final class AttachmentStore: @unchecked Sendable {
    let fileManager: FileManager
    let baseDirectory: URL

    init(fileManager: FileManager = .default, baseDirectory: URL) {
        self.fileManager = fileManager
        self.baseDirectory = baseDirectory
    }
}

extension AttachmentStore {
    static func uploads(fileManager: FileManager = .default) -> AttachmentStoring? {
        attachmentsInDocument(directory: "ChatUploads", fileManager: fileManager)
    }

    static func dowloads(fileManager: FileManager = .default) -> AttachmentStoring? {
        attachmentsInDocument(directory: "ChatDownloads", fileManager: fileManager)
    }

    static func attachmentsInDocument(directory: String, fileManager: FileManager = .default) -> AttachmentStoring? {
        let directory = fileManager
            .urls(
                for: .applicationSupportDirectory,
                in: .userDomainMask
            )
            .first?
            .appendingPathComponent(directory, isDirectory: true)

        guard let directory else {
            return nil
        }

        return AttachmentStore(fileManager: fileManager, baseDirectory: directory)
    }
}

extension AttachmentStore: AttachmentStoring {
    @discardableResult
    func store(attachment: Data, filename: String) throws -> URL {
        try createDirectoryIfNeeded()

        let fileURL = baseDirectory.appendingPathComponent(filename)

        try attachment.write(to: fileURL, options: .atomic)

        return fileURL
    }

    func loadAttachment(by filename: String) throws -> Data {
        let fileURL = baseDirectory.appendingPathComponent(filename)
        return try Data(contentsOf: fileURL)
    }

    func fileURL(for filename: String) -> URL {
        baseDirectory.appendingPathComponent(filename)
    }

    func hasFile(for filename: String) -> Bool {
        let fileURL = baseDirectory.appendingPathComponent(filename)

        var isDirectory: ObjCBool = false
        let isExists = fileManager.fileExists(atPath: fileURL.path, isDirectory: &isDirectory)

        return isExists && !isDirectory.boolValue
    }

    func createDirectoryIfNeeded() throws {
        var directory = baseDirectory

        if !fileManager.fileExists(atPath: directory.path) {
            try fileManager.createDirectory(at: directory, withIntermediateDirectories: true)

            // exclude attachments from icloud
            var values = URLResourceValues()
            values.isExcludedFromBackup = true
            try directory.setResourceValues(values)
        }
    }

    func remove(for filename: String) throws {
        let url = fileURL(for: filename)

        try fileManager.removeItem(at: url)
    }

    func createEmptyFile(for filename: String) throws -> URL {
        try createDirectoryIfNeeded()

        let url = fileURL(for: filename)
        fileManager.createFile(atPath: url.path, contents: nil)

        return url
    }

    func moveFile(from sourceFilename: String, to destinationFilename: String) throws {
        let sourceURL = fileURL(for: sourceFilename)
        let destinationURL = fileURL(for: destinationFilename)

        try fileManager.moveItem(at: sourceURL, to: destinationURL)
    }
}
