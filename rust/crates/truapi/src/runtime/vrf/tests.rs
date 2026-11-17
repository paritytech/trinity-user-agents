use super::{DOMAIN_2E11, DOMAIN_2E12, DOMAIN_2E16};
use verifiable::ring::RingDomainSize;

#[test]
fn domain_sizes_are_the_ones_verifiable_accepts() {
	assert_eq!(RingDomainSize::try_from(DOMAIN_2E11), Ok(RingDomainSize::Domain11));
	assert_eq!(RingDomainSize::try_from(DOMAIN_2E12), Ok(RingDomainSize::Domain12));
	assert_eq!(RingDomainSize::try_from(DOMAIN_2E16), Ok(RingDomainSize::Domain16));
}
