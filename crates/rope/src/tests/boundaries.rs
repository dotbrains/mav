use super::*;

#[test]
fn test_is_char_boundary() {
    let fixture = "地";
    let rope = Rope::from("地");
    for b in 0..=fixture.len() {
        assert_eq!(rope.is_char_boundary(b), fixture.is_char_boundary(b));
    }
    let fixture = "";
    let rope = Rope::from("");
    for b in 0..=fixture.len() {
        assert_eq!(rope.is_char_boundary(b), fixture.is_char_boundary(b));
    }
    let fixture = "🔴🟠🟡🟢🔵🟣⚫️⚪️🟤\n🏳️‍⚧️🏁🏳️‍🌈🏴‍☠️⛳️📬📭🏴🏳️🚩";
    let rope = Rope::from("🔴🟠🟡🟢🔵🟣⚫️⚪️🟤\n🏳️‍⚧️🏁🏳️‍🌈🏴‍☠️⛳️📬📭🏴🏳️🚩");
    for b in 0..=fixture.len() {
        assert_eq!(rope.is_char_boundary(b), fixture.is_char_boundary(b));
    }
}

#[test]
fn test_floor_char_boundary() {
    let fixture = "地";
    let rope = Rope::from("地");
    for b in 0..=fixture.len() {
        assert_eq!(rope.floor_char_boundary(b), fixture.floor_char_boundary(b));
    }

    let fixture = "";
    let rope = Rope::from("");
    for b in 0..=fixture.len() {
        assert_eq!(rope.floor_char_boundary(b), fixture.floor_char_boundary(b));
    }

    let fixture = "🔴🟠🟡🟢🔵🟣⚫️⚪️🟤\n🏳️‍⚧️🏁🏳️‍🌈🏴‍☠️⛳️📬📭🏴🏳️🚩";
    let rope = Rope::from("🔴🟠🟡🟢🔵🟣⚫️⚪️🟤\n🏳️‍⚧️🏁🏳️‍🌈🏴‍☠️⛳️📬📭🏴🏳️🚩");
    for b in 0..=fixture.len() {
        assert_eq!(rope.floor_char_boundary(b), fixture.floor_char_boundary(b));
    }
}

#[test]
fn test_ceil_char_boundary() {
    let fixture = "地";
    let rope = Rope::from("地");
    for b in 0..=fixture.len() {
        assert_eq!(rope.ceil_char_boundary(b), fixture.ceil_char_boundary(b));
    }

    let fixture = "";
    let rope = Rope::from("");
    for b in 0..=fixture.len() {
        assert_eq!(rope.ceil_char_boundary(b), fixture.ceil_char_boundary(b));
    }

    let fixture = "🔴🟠🟡🟢🔵🟣⚫️⚪️🟤\n🏳️‍⚧️🏁🏳️‍🌈🏴‍☠️⛳️📬📭🏴🏳️🚩";
    let rope = Rope::from("🔴🟠🟡🟢🔵🟣⚫️⚪️🟤\n🏳️‍⚧️🏁🏳️‍🌈🏴‍☠️⛳️📬📭🏴🏳️🚩");
    for b in 0..=fixture.len() {
        assert_eq!(rope.ceil_char_boundary(b), fixture.ceil_char_boundary(b));
    }
}
