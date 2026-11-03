@testable import polkadot_app

final class StubSearchContactInteractor: SearchContactInteractorInputProtocol {
    func setup() {}

    func search(username _: String) {}

    func decide(on _: ContactSearchPayload) {}
}
