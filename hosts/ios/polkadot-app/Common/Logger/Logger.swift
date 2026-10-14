import Foundation
import SwiftyBeaver
import UIKit
import MessageExchangeKit
import SDKLogger

// import DeviceKit

typealias LoggerProtocol = SDKLoggerProtocol

final class Logger: Sendable {
    static let shared = Logger()

    let log = SwiftyBeaver.self

    var minLevel: SwiftyBeaver.Level? {
        get {
            log.destinations.first?.minLevel
        }

        set {
            log.removeAllDestinations()

            if let level = newValue {
                let destination = ConsoleDestination()
                destination.minLevel = level
                log.addDestination(destination)
            }
        }
    }

    private init() {
        addConsoleDestination()
        addFileDestination()
        logSystemInfo()
    }
}

extension Logger: LoggerProtocol {
    func verbose(message: () -> String, file: String, function: String, line: Int) {
        log.custom(
            level: .verbose,
            message: message(),
            file: file,
            function: function,
            line: line
        )
    }

    func debug(message: () -> String, file: String, function: String, line: Int) {
        log.custom(
            level: .debug,
            message: message(),
            file: file,
            function: function,
            line: line
        )
    }

    func info(message: () -> String, file: String, function: String, line: Int) {
        log.custom(
            level: .info,
            message: message(),
            file: file,
            function: function,
            line: line
        )
    }

    func warning(message: () -> String, file: String, function: String, line: Int) {
        log.custom(
            level: .warning,
            message: message(),
            file: file,
            function: function,
            line: line
        )
    }

    func error(message: () -> String, file: String, function: String, line: Int) {
        log.custom(
            level: .error,
            message: message(),
            file: file,
            function: function,
            line: line
        )
    }
}

// MARK: - Log destinations

private extension Logger {
    func addConsoleDestination() {
        let destination = ConsoleDestination()
        destination.minLevel = EnviromentVariables.isDebugEnabled ? .verbose : .info
        log.addDestination(destination)
    }

    func addFileDestination() {
        #if TESTNET_FEATURE
            guard let directoryURL = FileManager.default.appLogsDirectoryURL() else {
                return
            }

            FileManager.default.cleanupLogDirectory(at: directoryURL)
            let logFileURL = FileManager.default.newLogFileURL(inDirectory: directoryURL)
            log.addDestination(FileDestination(logFileURL: logFileURL))
        #endif
    }
}

// MARK: - System info

private extension Logger {
    func logSystemInfo() {
        let appVersion = Bundle.main.infoDictionary?["CFBundleShortVersionString"] ?? ""
        let buildVersion = Bundle.main.infoDictionary?["CFBundleVersion"] ?? ""
        log.debug("App Version: \(appVersion) \(buildVersion)")

        log.debug("OS: \(UIDevice.current.systemName) \(UIDevice.current.systemVersion)")
        log.debug("Device: \(deviceModelIdentifier)")
    }

    var deviceModelIdentifier: String {
        var systemInfo = utsname()
        uname(&systemInfo)

        return withUnsafePointer(to: &systemInfo.machine) {
            $0.withMemoryRebound(to: CChar.self, capacity: 1) {
                String(cString: $0)
            }
        }
    }
}
