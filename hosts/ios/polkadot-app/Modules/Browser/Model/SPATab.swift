import Foundation
import Products

struct SPATab: Equatable {
    let id: UUID
    let dotDomain: String
    var page: String?
    let createdAt: Date
}

extension SPATab {
    init(dotDomain: String, page: String? = nil, now: Date = Date()) {
        self.init(
            id: UUID(),
            dotDomain: dotDomain,
            page: page,
            createdAt: now
        )
    }

    func makeProductPage(hostProvider: ProductHostProviding) -> ProductPage? {
        guard let host = hostProvider.host(rawString: dotDomain) else { return nil }
        return ProductPage(host: host, page: page)
    }
}
