import Foundation

extension NSItemProvider {
    func moveRepresentationToTempDirectory(
        forTypeIdentifier: String
    ) async throws -> URL? {
        try await withCheckedThrowingContinuation { continuation in
            self.loadFileRepresentation(
                forTypeIdentifier: forTypeIdentifier
            ) { url, error in
                if let error {
                    continuation.resume(throwing: error)
                    return
                }

                guard let sourceUrl = url else {
                    continuation.resume(returning: nil)
                    return
                }

                do {
                    let fileManager = FileManager.default
                    let destinationURL = fileManager.temporaryDirectory
                        .appendingPathComponent(UUID().uuidString)
                        .appendingPathExtension(sourceUrl.pathExtension)

                    if fileManager.fileExists(atPath: destinationURL.path) {
                        try fileManager.removeItem(at: destinationURL)
                    }

                    try fileManager.copyItem(at: sourceUrl, to: destinationURL)

                    continuation.resume(returning: destinationURL)
                } catch {
                    continuation.resume(throwing: error)
                }
            }
        }
    }
}
