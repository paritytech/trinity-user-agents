import BulletinChain

extension HexToCIDConverter {
    convenience init() {
        self.init(ipfsBaseURL: AppConfig.KnownIPFS.main)
    }
}
