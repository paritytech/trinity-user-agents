import Foundation
import WebKit

/// Records the bytes a WebKit scheme task is answered with.
final class StubSchemeTask: NSObject, WKURLSchemeTask {
    let request: URLRequest
    private(set) var servedData = Data()

    init(request: URLRequest) {
        self.request = request
    }

    func didReceive(_: URLResponse) {}

    func didReceive(_ data: Data) {
        servedData.append(data)
    }

    func didFinish() {}

    func didFailWithError(_: Error) {}
}
