use super::*;

#[test]
fn a_payment_topic_never_equals_a_refund_topic() {
	let receivable = Receivable([4; 32]);
	assert_ne!(payment_topic(receivable), refund_topic(receivable));
	assert_ne!(payment_topic(receivable), payment_topic(Receivable([5; 32])));
	assert_eq!(payment_topic(receivable), payment_topic(receivable));
}

#[test]
fn a_topic_hashes_its_label_and_the_receivable() {
	let receivable = Receivable([4; 32]);
	let expected = blake2_256(&[b"coinage/payment-topic".as_slice(), &[4; 32]].concat());
	assert_eq!(payment_topic(receivable), Topic(expected));
}
