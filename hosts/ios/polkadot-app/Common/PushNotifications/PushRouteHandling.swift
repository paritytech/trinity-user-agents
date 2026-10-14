import Foundation

protocol PushRouteHandling: AnyObject {
    func handle(route: PushNavigationRoute) -> Bool
}
