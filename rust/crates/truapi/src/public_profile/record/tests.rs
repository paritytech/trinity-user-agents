use super::*;

const CID: &str = "bafk2bzaceaf5do2x4p46vaazk2txb7qqnd4vaobvi6wkixq5hu2u2v6qctfqy";

fn avatar(cid: &str) -> ProfileAvatar {
	ProfileAvatar { cid: cid.parse().unwrap(), format: ImageFormat::Png }
}

#[test]
fn avatar_records_encode_the_digest_and_format() {
	for (text, format) in [
		("BIgAC9G7V+P56oAZVqdw/hBo+VA4NUespF4dPTVNV9AUywwA", ImageFormat::Png),
		("BIgAC9G7V+P56oAZVqdw/hBo+VA4NUespF4dPTVNV9AUywwB", ImageFormat::Jpeg),
		("BIgAC9G7V+P56oAZVqdw/hBo+VA4NUespF4dPTVNV9AUywwC", ImageFormat::Webp),
	] {
		let record = ProfileRecord::from_text(text).expect("record decodes");

		assert_eq!(
			(record.to_text().unwrap(), record.avatar()),
			(text.to_string(), Some(ProfileAvatar { cid: CID.parse().unwrap(), format })),
		);
	}
}

#[test]
fn an_incomplete_avatar_is_preserved_and_skipped() {
	for length in [0, 31, 32] {
		let invalid_avatar = [vec![0u8], vec![0; length]].concat();
		let record = ProfileRecord {
			entries: vec![
				WrapperKeepOpaque::from_encoded(invalid_avatar),
				WrapperKeepOpaque::from_value(ProfileEntry::Avatar(avatar(CID))),
			],
		};
		let read =
			ProfileRecord::from_text(&record.to_text().unwrap()).expect("opaque record decodes");

		assert_eq!((read.encode(), read.avatar()), (record.encode(), Some(avatar(CID))));
	}
}

#[test]
fn the_wire_record_preserves_length_prefixed_entries() {
	let encoded = hex::decode(
		"080807aa88000bd1bb57e3f9ea801956a770fe1068f950383547aca45e1d3d354d57d014cb0c00",
	)
	.unwrap();
	let record = ProfileRecord::from_text(&STANDARD.encode(&encoded)).expect("record decodes");

	assert_eq!((record.encode(), record.avatar()), (encoded, Some(avatar(CID))));
}

#[test]
fn an_extended_avatar_entry_is_preserved_for_newer_readers() {
	let encoded =
		hex::decode("048c000bd1bb57e3f9ea801956a770fe1068f950383547aca45e1d3d354d57d014cb0c00aa")
			.unwrap();
	let record = ProfileRecord::from_text(&STANDARD.encode(&encoded)).expect("record decodes");

	assert_eq!((record.encode(), record.avatar()), (encoded, None));
}

#[test]
fn an_entry_kind_from_a_newer_writer_is_kept_and_skipped() {
	let newer_kind = [vec![7u8], "Alice".encode()].concat();
	let record = ProfileRecord {
		entries: vec![
			WrapperKeepOpaque::from_encoded(newer_kind.clone()),
			WrapperKeepOpaque::from_value(ProfileEntry::Avatar(avatar(CID))),
		],
	};

	let read = ProfileRecord::from_text(&record.to_text().unwrap()).expect("record decodes");

	assert_eq!(read, record);
	assert_eq!(read.avatar(), Some(avatar(CID)));
	assert_eq!(read.encode(), record.encode(), "a writer rewrites the unknown entry unchanged");
}

#[test]
fn a_new_avatar_replaces_the_one_readers_show_and_keeps_other_entries() {
	let entry = |seed: u8| {
		WrapperKeepOpaque::from_value(ProfileEntry::Avatar(ProfileAvatar {
			cid: [seed; 32].into(),
			format: ImageFormat::Png,
		}))
	};
	let unknown = WrapperKeepOpaque::from_encoded(vec![7, 1, 2]);
	let mut record = ProfileRecord { entries: vec![unknown.clone(), entry(1), entry(2)] };

	record.set_avatar(ProfileAvatar { cid: [3; 32].into(), format: ImageFormat::Png });

	assert_eq!(record, ProfileRecord { entries: vec![entry(3), unknown, entry(2)] });
}

#[test]
fn the_first_avatar_wins() {
	let record = ProfileRecord {
		entries: vec![
			WrapperKeepOpaque::from_value(ProfileEntry::Avatar(avatar(CID))),
			WrapperKeepOpaque::from_value(ProfileEntry::Avatar(ProfileAvatar {
				cid: [2; 32].into(),
				format: ImageFormat::Png,
			})),
		],
	};

	assert_eq!(record.avatar(), Some(avatar(CID)));
}

#[test]
fn an_avatar_in_an_unknown_format_is_no_avatar() {
	let unknown_format =
		[vec![0u8], CID.parse::<BulletinCid>().unwrap().encode(), vec![9u8]].concat();
	let record = ProfileRecord { entries: vec![WrapperKeepOpaque::from_encoded(unknown_format)] };

	let read = ProfileRecord::from_text(&record.to_text().unwrap()).expect("record decodes");

	assert_eq!(read.avatar(), None);
}

#[test]
fn a_malformed_record_is_refused() {
	let record = ProfileRecord {
		entries: vec![WrapperKeepOpaque::from_value(ProfileEntry::Avatar(avatar(CID)))],
	};
	let with_trailing_bytes = STANDARD.encode([record.encode(), vec![0]].concat());
	let oversized =
		ProfileRecord { entries: vec![WrapperKeepOpaque::from_encoded(vec![0; MAX_RECORD_BYTES])] };

	assert_eq!(oversized.to_text(), None, "a writer never publishes what readers refuse");
	assert_eq!(ProfileRecord::from_text(&with_trailing_bytes), None);
	assert_eq!(ProfileRecord::from_text(&STANDARD.encode(oversized.encode())), None);
	assert_eq!(ProfileRecord::from_text("not base64!"), None);
}

#[test]
fn shared_formats_keep_the_avatar_wire_indices() {
	assert_eq!(
		[ImageFormat::Png, ImageFormat::Jpeg, ImageFormat::Webp, ImageFormat::Gif].map(|format| (
			format.encode(),
			ImageFormat::decode_all(&mut format.encode().as_slice()).unwrap()
		)),
		[
			(vec![0], ImageFormat::Png),
			(vec![1], ImageFormat::Jpeg),
			(vec![2], ImageFormat::Webp),
			(vec![3], ImageFormat::Gif),
		]
	);
}

#[test]
fn a_gif_avatar_uses_the_shared_image_format() {
	let text = "BIgAC9G7V+P56oAZVqdw/hBo+VA4NUespF4dPTVNV9AUywwD";
	let record = ProfileRecord::from_text(text).expect("GIF record decodes");

	assert_eq!(
		(record.to_text().unwrap(), record.avatar()),
		(
			text.to_string(),
			Some(ProfileAvatar { cid: CID.parse().unwrap(), format: ImageFormat::Gif })
		),
	);
}
