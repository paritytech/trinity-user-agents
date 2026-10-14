import Foundation

extension ChatRemoteMessageContent.NodeEndpoint {
    func toURL() throws -> URL {
        switch self {
        case let .wssUrl(string):
            guard let url = URL(string: string) else {
                throw HOPFileLoaderError.invalidUrl
            }
            return url
        }
    }
}
