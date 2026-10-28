use super::*;

#[test]
fn string_inputs_select_assets_by_id_before_calculating_a_swap() {
    let reserves = r#"[
        {"asset_id": 11, "amount": "5000000000000000000000", "decimals": 18},
        {"asset_id": 3, "amount": "1000000000", "decimals": 6},
        {"asset_id": 7, "amount": "3000000000", "decimals": 6}
    ]"#;
    let quote = |pegs: &str| {
        calculate_out_given_in(
            reserves.to_string(),
            7,
            11,
            "1000000".to_string(),
            "1000".to_string(),
            "0.01".to_string(),
            pegs.to_string(),
        )
    };

    assert_eq!(quote(r#"[["1", "1"], ["1", "1"], ["1", "1"]]"#), "990710823773957149");
    assert_eq!(quote(""), "-1");
}
