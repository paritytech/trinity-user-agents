import Foundation

@MainActor
public protocol ProductPermissionRouting: ProductsRouting {
    func showPrompt(context: ProductPermissionContext)
}
