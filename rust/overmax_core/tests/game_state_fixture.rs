use overmax_core::GameSessionState;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct FixtureCase {
    name: String,
    state: FixtureState,
    expected: ExpectedState,
}

#[derive(Debug, Deserialize)]
struct FixtureState {
    song_id: Option<u32>,
    mode: Option<String>,
    diff: Option<String>,
    is_stable: bool,
    is_max_combo: bool,
    rate: Option<f32>,
}

#[derive(Debug, Deserialize)]
struct ExpectedState {
    is_valid: bool,
    should_store_rate: bool,
    display: String,
}

#[test]
fn game_state_matches_python_reference_fixture() {
    let cases: Vec<FixtureCase> =
        serde_json::from_str(include_str!("../../../test/fixtures/game_state_cases.json"))
            .expect("game state fixture must be valid JSON");

    for case in cases {
        let state = build_state(case.state);
        assert_eq!(state.is_valid(), case.expected.is_valid, "{}", case.name);
        assert_eq!(
            state.should_store_rate(),
            case.expected.should_store_rate,
            "{}",
            case.name
        );
        assert_eq!(state.to_string(), case.expected.display, "{}", case.name);
    }
}

fn build_state(input: FixtureState) -> GameSessionState {
    GameSessionState {
        song_id: input.song_id,
        mode: input.mode,
        diff: input.diff,
        is_stable: input.is_stable,
        is_max_combo: input.is_max_combo,
        rate: input.rate,
    }
}
