//! `TB` reply format from the Elecraft K3S/K3/KX3 Programmer's Reference (rev F6):
//! `TBtrrs;` where t = chars still to send (0-9), rr = received char count (00-40), s = text.

use kx3_cw::{TbError, parse_tb};

#[test]
fn empty_buffers() {
    let reply = parse_tb("TB000;").unwrap();
    assert_eq!(reply.tx_pending, 0);
    assert_eq!(reply.text, "");
}

#[test]
fn received_text_and_pending_transmit_count() {
    let reply = parse_tb("TB305HELLO;").unwrap();
    assert_eq!(reply.tx_pending, 3);
    assert_eq!(reply.text, "HELLO");
}

#[test]
fn text_keeps_its_spaces() {
    let reply = parse_tb("TB009CQ CQ DE ;").unwrap();
    assert_eq!(reply.text, "CQ CQ DE ");
}

#[test]
fn semicolons_inside_text_are_kept_because_the_count_says_how_long_it_is() {
    let reply = parse_tb("TB003A;B;").unwrap();
    assert_eq!(reply.text, "A;B");
}

#[test]
fn rejects_malformed_replies() {
    for bad in [
        "",
        "XX000;",
        "TB30;",
        "TB000",
        "TB005AB;",
        "TBx00;",
        "TB0ab;",
        "TB002ABC;",
    ] {
        assert!(
            matches!(parse_tb(bad), Err(TbError { .. })),
            "{bad:?} should be rejected"
        );
    }
}
