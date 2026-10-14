import Products

extension ProductsRouting {
    func showActionConfirmation(context: TrUAPIActionConfirmationContext) {
        let view = TrUAPIActionPromptViewFactory.createView(context: context)
        if !present(view: view) {
            context.deliver(false)
        }
    }
}
