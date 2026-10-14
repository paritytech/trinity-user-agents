import CID
import CarParser
import Foundation
import FoundationExt

public enum CarFetcherError: Error {
    case invalidContentHash
    case notCarArchive
}

public protocol CarFetcherProtocol {
    /// Downloads the CAR archive to a temporary file. The caller owns the returned file and must delete it once
    /// unpacked.
    /// `onProgress` reports `(downloaded, total)` bytes; `total` is nil when the gateway sent no
    /// Content-Length.
    func fetchCarToFile(
        contentHash: Data,
        onProgress: @escaping (_ downloaded: Int64, _ total: Int64?) -> Void
    ) async throws -> URL
}

public final class CarFetcher: CarFetcherProtocol {
    enum Constants {
        static let fileFlushStepBytes = 64 * 1_024
        static let progressReportStepBytes: Int64 = 256 * 1_024
        // Buffer this many leading bytes before running `validatePrefix`; ample for a CAR header.
        static let prefixProbeBytes = 64 * 1_024
    }

    private let gatewayBaseUrl: URL
    private let session: URLSession
    private let fileManager: FileManager
    private let temporaryDirectory: URL

    public init(
        gatewayBaseUrl: URL,
        session: URLSession = .shared,
        fileManager: FileManager = .default,
        temporaryDirectory: URL? = nil
    ) {
        self.gatewayBaseUrl = gatewayBaseUrl
        self.session = session
        self.fileManager = fileManager
        self.temporaryDirectory = temporaryDirectory ?? fileManager.temporaryDirectory
    }

    public func fetchCarToFile(
        contentHash: Data,
        onProgress: @escaping (_ downloaded: Int64, _ total: Int64?) -> Void
    ) async throws -> URL {
        guard let cid = try? CID(contentHash) else {
            throw CarFetcherError.invalidContentHash
        }

        let cidString = cid.toBaseEncodedString

        guard let rawUrl = URL(string: cidString, relativeTo: gatewayBaseUrl) else {
            throw CarFetcherError.invalidContentHash
        }

        // Phase 1: stream the raw CID, validating the leading bytes as they arrive. A legacy CID
        // *is* an uploaded CAR file. A directory CID is not — the download throws `.notCarArchive`
        // right after the probe (a few KB) instead of fetching the whole archive to discard it.
        do {
            let prefixValidator: (Data) throws -> Void = {
                guard CarParser.looksLikeCarArchive($0) else {
                    throw CarFetcherError.notCarArchive
                }
            }

            return try await downloadToFile(
                from: URLRequest(url: rawUrl),
                onProgress: onProgress,
                validatePrefix: prefixValidator
            )
        } catch CarFetcherError.notCarArchive {
            // Phase 2: directory CID — fetch it exported as a CAR archive.
            let formatUrl = rawUrl.appending(queryItems: [URLQueryItem(name: "format", value: "car")])
            var request = URLRequest(url: formatUrl)
            request.setValue("application/vnd.ipld.car", forHTTPHeaderField: "Accept")

            return try await downloadToFile(from: request, onProgress: onProgress)
        }
    }
}

private extension CarFetcher {
    // Streams the response body to a temporary file, flushing in fixed-size chunks so peak heap
    // stays flat regardless of archive size. When `validatePrefix` is supplied, it runs against the
    // leading bytes before anything past the probe is persisted; if it throws, the download stops
    // there (only the probe has been read) and the partial temp file is removed.
    func downloadToFile(
        from request: URLRequest,
        onProgress: @escaping (_ downloaded: Int64, _ total: Int64?) -> Void,
        validatePrefix: ((_ prefix: Data) throws -> Void)? = nil
    ) async throws -> URL {
        let (bytes, response) = try await session.bytes(for: request)
        try response.ensureSuccess()

        let expected = response.expectedContentLength
        let total: Int64? = expected > 0 ? expected : nil

        let tempURL = temporaryDirectory
            .appendingPathComponent("dotns_car_\(UUID().uuidString).car")
        fileManager.createFile(atPath: tempURL.path, contents: nil)

        let handle = try FileHandle(forWritingTo: tempURL)

        var buffer = [UInt8]()
        buffer.reserveCapacity(Constants.fileFlushStepBytes)
        var downloaded: Int64 = 0
        var lastReported: Int64 = 0
        var pendingValidation = validatePrefix

        do {
            // URLSession.AsyncBytes yields UInt8; buffering in [UInt8] avoids per-byte Data mutation cost.
            for try await byte in bytes {
                buffer.append(byte)

                if let validate = pendingValidation, buffer.count >= Constants.prefixProbeBytes {
                    try validate(Data(buffer))
                    pendingValidation = nil
                }

                guard pendingValidation == nil, buffer.count >= Constants.fileFlushStepBytes else { continue }
                try downloaded += Int64(flush(&buffer, to: handle))

                guard downloaded - lastReported >= Constants.progressReportStepBytes else { continue }
                onProgress(downloaded, total)
                lastReported = downloaded
            }

            if let pendingValidation {
                try pendingValidation(Data(buffer))
            }

            try downloaded += Int64(flush(&buffer, to: handle))
            try handle.close()
        } catch {
            try? handle.close()
            try? fileManager.removeItem(at: tempURL)
            throw error
        }

        onProgress(downloaded, total)
        return tempURL
    }

    /// Writes the buffered bytes to `handle`, clears the buffer (keeping capacity), and returns the count written.
    private func flush(_ buffer: inout [UInt8], to handle: FileHandle) throws -> Int {
        guard !buffer.isEmpty else { return 0 }
        try handle.write(contentsOf: Data(buffer))
        let written = buffer.count
        buffer.removeAll(keepingCapacity: true)
        return written
    }
}
