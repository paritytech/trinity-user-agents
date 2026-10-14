import NovaCrypto

struct AccountCreateMetadata {
    let mnemonic: IRMnemonicProtocol

    var words: [String] {
        mnemonic.allWords()
    }
}
