import Foundation

enum LinkHandlingOutcome {
    case skipped
    case handled
    case failed(Error)
}

protocol LinkHandler {
    func handle(_ url: URL) -> LinkHandlingOutcome
}
