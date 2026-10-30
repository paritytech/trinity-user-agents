import Foundation
import Products

enum ProductContentServingError: Error {
    case executableNotStaged(ExecutableKind)
}

extension ProductContent {
    func served(for kind: ExecutableKind) throws -> DotNsResolvedContent {
        guard let content = content(for: kind) else {
            throw ProductContentServingError.executableNotStaged(kind)
        }

        return content
    }
}
