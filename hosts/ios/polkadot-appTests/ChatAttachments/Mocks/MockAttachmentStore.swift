import Foundation

@testable import polkadot_app

final class MockAttachmentStore: AttachmentStoring, @unchecked Sendable {
    private var files: [String: Data] = [:]

    @discardableResult
    func store(attachment: Data, filename: String) throws -> URL {
        files[filename] = attachment
        return fileURL(for: filename)
    }

    func loadAttachment(by filename: String) throws -> Data {
        guard let data = files[filename] else {
            throw NSError(domain: "MockAttachmentStore", code: 0, userInfo: [
                NSLocalizedDescriptionKey: "File not found: \(filename)"
            ])
        }
        return data
    }

    func createDirectoryIfNeeded() throws {}

    func fileURL(for filename: String) -> URL {
        URL(fileURLWithPath: "/mock/\(filename)")
    }

    func hasFile(for filename: String) -> Bool {
        files[filename] != nil
    }

    func remove(for filename: String) throws {
        files.removeValue(forKey: filename)
    }

    @discardableResult
    func createEmptyFile(for filename: String) throws -> URL {
        if files[filename] == nil {
            files[filename] = Data()
        }
        return fileURL(for: filename)
    }

    func moveFile(from sourceFilename: String, to destinationFilename: String) throws {
        guard let data = files.removeValue(forKey: sourceFilename) else {
            throw NSError(domain: "MockAttachmentStore", code: 0, userInfo: [
                NSLocalizedDescriptionKey: "Source not found: \(sourceFilename)"
            ])
        }
        files[destinationFilename] = data
    }
}
