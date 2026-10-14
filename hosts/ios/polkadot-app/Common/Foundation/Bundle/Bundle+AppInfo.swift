import Foundation

extension Bundle {
    var appVersion: String? {
        infoDictionary?[.appVersion] as? String
    }

    var appBuild: String? {
        infoDictionary?[.buildVersion] as? String
    }
}

// MARK: - Constants

private extension String {
    static let appVersion = "CFBundleShortVersionString"
    static let buildVersion = "CFBundleVersion"
}
