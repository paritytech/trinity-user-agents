protocol ProductsSignConfirmContextProtocol: AnyObject {
    var requester: PolkadotSigningRequester { get }
    var input: ProductsSignConfirmInput { get }

    /// Resume the awaiting core call with the user's decision; called from the
    /// wireframe once the sheet has finished dismissing.
    @MainActor func deliver(_ decision: Bool)
}

protocol ProductsSignConfirmInteractorInputProtocol: AnyObject {
    func setup()
}

@MainActor
protocol ProductsSignConfirmInteractorOutputProtocol: AnyObject {
    func didStartParsingRequest()
    func didFinishParsingRequest(with model: ProductsSignConfirmModel)
    func didFailToParseRequest(with error: Error)
}

@MainActor
protocol ProductsSignConfirmWireframeProtocol: PolkadotSigningDetailsPresentable {
    func hide(view: PolkadotSigningViewProtocol?, decision: Bool)
}
