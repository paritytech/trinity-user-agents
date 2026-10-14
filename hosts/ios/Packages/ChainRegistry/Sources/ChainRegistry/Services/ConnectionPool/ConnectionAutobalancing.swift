import Foundation

public protocol ConnectionAutobalancing {
    var urls: [URL] { get }

    func changeUrls(_ newUrls: [URL])
}
