// Copyright: Anki 3DS contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use ctru::prelude::*;

#[allow(dead_code)]
#[path = "../../../rslib/src/text.rs"]
mod text;

fn verify_original_text() {
    assert_eq!(text::strip_html("t<b>e</b>st"), "test");
    assert_eq!(text::strip_html("so<SCRIPT>ignored</script>me"), "some");
    assert_eq!(text::decode_entities("&lt;Anki&gt; &amp; 3DS"), "<Anki> & 3DS");
    assert!(text::is_html("<b>Anki</b>"));
    assert!(!text::is_html("Anki"));
    assert_eq!(text::normalize_to_nfc("e\u{301}"), "é");
}

fn main() {
    let apt = Apt::new().expect("APT initialization failed");
    let mut hid = Hid::new().expect("HID initialization failed");
    let gfx = Gfx::new().expect("Graphics initialization failed");
    let console = Console::new(gfx.top_screen.borrow_mut());

    verify_original_text();
    console.clear();

    while apt.main_loop() {
        gfx.wait_for_vblank();
        println!("\x1b[1;1HAnki original text module");
        println!("ARMv6K / Old 3DS target");
        println!("PASS: 6 assertions");
        println!("HTML / entities / Unicode NFC");
        println!("Isolated text regression test.");
        println!("Press START to exit.");
        hid.scan_input();
        if hid.keys_down().contains(KeyPad::START) {
            break;
        }
    }
}
