import Foundation

enum APNSTokenProviderFacade {
    static let sharedManager: any APNSTokenManaging = APNSTokenProvider(
        logger: Logger.shared
    )
}
