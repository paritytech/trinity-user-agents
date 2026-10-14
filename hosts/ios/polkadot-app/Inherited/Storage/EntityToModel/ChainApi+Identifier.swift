import Foundation
import ChainRegistry

extension CDChainApi {
    var identifier: String {
        LocalChainExternalApi.createId(
            from: apiType!,
            serviceType: serviceType!,
            url: url!
        )
    }
}
