extension TabBarBottomChromeController {
    func handleAction(_ action: TabBarAction) {
        switch action {
        case .connectionStatus:
            #if FEATURE_PRODUCTS
                onConnectionStatusTapped?()
            #else
                panelController.togglePanel(.shelf)
            #endif
        case .spaTabs:
            panelController.togglePanel(.shelf)
        case .scan:
            panelController.togglePanel(.content(.scan))
        }
    }
}
