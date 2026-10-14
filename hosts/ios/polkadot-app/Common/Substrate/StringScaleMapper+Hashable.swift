import SubstrateSdk

extension StringScaleMapper: @retroactive Hashable where T: Hashable {
    public func hash(into hasher: inout Hasher) {
        hasher.combine(value)
    }
}
