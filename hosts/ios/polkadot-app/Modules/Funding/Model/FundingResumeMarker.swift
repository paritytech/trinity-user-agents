import Foundation

/// Whether a provider still worked on a funding session when the app last
/// heard from the core. The next launch starts the core at once when it did,
/// so the providers carry on from their last checkpoint instead of waiting
/// for a screen or product to need the core.
enum FundingResumeMarker {
    private static let key = "funding.inProgress"

    static var inProgress: Bool {
        UserDefaults.standard.bool(forKey: key)
    }

    static func record(_ runtime: FundingRuntime) async {
        guard let inProgress = try? await runtime.fundingInProgress() else { return }
        UserDefaults.standard.set(inProgress, forKey: key)
    }
}
