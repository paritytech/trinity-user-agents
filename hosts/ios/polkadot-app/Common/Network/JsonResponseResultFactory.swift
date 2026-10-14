import Foundation

final class JsonResponseResultFactory<T: Decodable>: BaseNetworkResultFactory<T> {
    private let decoder: JSONDecoder

    init(decoder: JSONDecoder = JSONDecoder()) {
        self.decoder = decoder
        super.init()
    }

    override func parse(data: Data) throws -> T {
        try decoder.decode(T.self, from: data)
    }
}
