//! B6: the `BigInteger` parity scenarios are inputs only and cover every operation, with zero,
//! negative operands and values above 2^1024. (The value-level checks of B2-B5 against `i128`
//! and the algebraic identities live in the other `big_integer*.rs` test files.)

const SCHEMA: &str =
    include_str!("../../../../parity/scenarios/maths/big_integer/scenario.schema.json");
const SCENARIOS: [&str; 5] = [
    include_str!("../../../../parity/scenarios/maths/big_integer/arithmetic.json"),
    include_str!("../../../../parity/scenarios/maths/big_integer/bits.json"),
    include_str!("../../../../parity/scenarios/maths/big_integer/number_theory.json"),
    include_str!("../../../../parity/scenarios/maths/big_integer/sign_and_conversion.json"),
    include_str!("../../../../parity/scenarios/maths/big_integer/text_and_bytes.json"),
];

/// The op names listed in the schema's `enum`, read line by line (CRLF-safe).
fn schema_ops() -> Vec<String> {
    let lines: Vec<&str> = SCHEMA.lines().collect();
    let start = lines
        .iter()
        .position(|l| l.contains("\"enum\": ["))
        .expect("op enum");
    let start = start
        + lines[start..]
            .iter()
            .position(|l| l.contains("\"add\""))
            .expect("ops");
    lines[start..]
        .iter()
        .take_while(|l| !l.trim_start().starts_with(']'))
        .flat_map(|l| l.split(','))
        .map(|s| s.trim().trim_matches('"').to_owned())
        .filter(|s| !s.is_empty())
        .collect()
}

#[test]
fn b6_scenarios_cover_every_operation() {
    let ops = schema_ops();
    assert!(ops.len() >= 38, "{ops:?}");
    for op in &ops {
        let needle = format!("\"op\": \"{op}\"");
        assert!(
            SCENARIOS.iter().any(|s| s.contains(&needle)),
            "no scenario step uses {op}"
        );
    }
}

#[test]
fn b6_scenarios_hold_inputs_only_with_edge_values() {
    for s in SCENARIOS {
        assert!(!s.contains("\"expect\""), "scenario files hold inputs only");
        assert!(s.contains("\"juce_tag\": \"9.0.4\"") && s.contains("\"juce_core/BigInteger\""));
        let texts: Vec<&str> = s
            .lines()
            .filter_map(|l| l.split("\"text\": \"").nth(1))
            .filter_map(|t| t.split('"').next())
            .collect();
        assert!(texts.contains(&"0"), "a zero operand");
        assert!(
            texts.iter().any(|t| t.starts_with('-')),
            "a negative operand"
        );
        assert!(
            texts.iter().any(|t| t.trim_start_matches('-').len() > 256),
            "a value above 2^1024"
        );
        for t in texts {
            assert!(syrp_maths::BigInteger::parse(t, 16).is_ok(), "{t}");
        }
    }
}
