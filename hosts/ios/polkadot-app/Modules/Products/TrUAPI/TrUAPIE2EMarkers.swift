import Foundation

#if targetEnvironment(simulator)
    /// Files the truapi E2E harness polls to observe the app from outside.
    enum TrUAPIE2EMarkers {
        /// `ProcessInfo.environment` rebuilds the whole dictionary on every read, so
        /// the harness switch is resolved once.
        static let isEnabled =
            ProcessInfo.processInfo.environment["TRUAPI_IOS_E2E_RUNTIME_MARKERS"] == "1"

        private static let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("truapi-e2e", isDirectory: true)

        static func write(_ name: String, logger: LoggerProtocol) {
            guard isEnabled else { return }

            do {
                try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
                try Data().write(to: directory.appendingPathComponent(name), options: .atomic)
            } catch {
                logger.warning("Failed to write TrUAPI E2E marker \(name): \(error)")
            }
        }

        static func exists(_ name: String) -> Bool {
            FileManager.default.fileExists(atPath: directory.appendingPathComponent(name).path)
        }
    }
#endif
