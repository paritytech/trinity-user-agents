use futures::executor::block_on;

use super::*;
use crate::coinage::payments::testing::Board;

fn cheque(amount: u32) -> CoinPaymentCheque {
	CoinPaymentCheque { id: [1; 32], amount, encrypted_secrets: vec![1, 2, 3] }
}

#[test]
fn a_statement_holds_the_cheque_inline_and_other_items_are_ignored() {
	let board = Board::default();
	let topic = Topic([5; 32]);
	block_on(post(&board, "shop", topic, &cheque(8), 100)).unwrap();
	block_on(board.post("shop", topic, vec![9, 9], 100)).unwrap();
	block_on(board.post("shop", Topic([6; 32]), ChannelItem::Inline(cheque(4)).encode(), 100)).unwrap();
	assert_eq!(board.posts.lock()[0].2, [vec![0u8], cheque(8).encode()].concat(), "the variant byte, then the cheque");
	assert_eq!(board.posts.lock()[0].0, "shop", "signed as the product");
	assert_eq!(block_on(fetch(&board, topic)), vec![cheque(8)]);
}
