import CID

extension CID: @retroactive Hashable {
    public func hash(into hasher: inout Hasher) {
        hasher.combine(rawData)
    }
}
