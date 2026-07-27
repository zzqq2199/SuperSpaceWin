//! Key-name to Windows virtual-key mapping.
//!
//! Names are kept compatible with the macOS version's config.json
//! (`key_codes.py`): e.g. `delete` means backspace, `forward_delete`
//! means the Del key, `command` is translated to Ctrl, `option` to Alt.

/// Sentinel used as the mapping target that quits the program (mac: `exit = -1`).
pub const EXIT_KEY: u16 = 0xFFFF;

pub const VK_SPACE: u16 = 0x20;

pub fn vk_from_name(name: &str) -> Option<u16> {
    let vk = match name {
        "exit" => EXIT_KEY,

        // Letters (VK 'A'..'Z')
        n if n.len() == 1 && n.as_bytes()[0].is_ascii_lowercase() => {
            (n.as_bytes()[0].to_ascii_uppercase()) as u16
        }

        // Number row, mac names k_0..k_9
        "k_0" => 0x30, "k_1" => 0x31, "k_2" => 0x32, "k_3" => 0x33, "k_4" => 0x34,
        "k_5" => 0x35, "k_6" => 0x36, "k_7" => 0x37, "k_8" => 0x38, "k_9" => 0x39,

        // Function keys
        "f1" => 0x70, "f2" => 0x71, "f3" => 0x72, "f4" => 0x73,
        "f5" => 0x74, "f6" => 0x75, "f7" => 0x76, "f8" => 0x77,
        "f9" => 0x78, "f10" => 0x79, "f11" => 0x7A, "f12" => 0x7B,
        "f13" => 0x7C, "f14" => 0x7D, "f15" => 0x7E, "f16" => 0x7F,

        // Control keys (mac semantics: delete = backspace)
        "space" => VK_SPACE,
        "tab" => 0x09,
        "return_key" | "enter" => 0x0D,
        "escape" => 0x1B,
        "delete" | "backspace" => 0x08,
        "forward_delete" | "del" => 0x2E,
        "caps_lock" => 0x14,

        // Navigation
        "left_arrow" => 0x25,
        "up_arrow" => 0x26,
        "right_arrow" => 0x27,
        "down_arrow" => 0x28,
        "home" => 0x24,
        "end" => 0x23,
        "page_up" => 0x21,
        "page_down" => 0x22,
        "insert" => 0x2D,

        // Punctuation (OEM keys, US layout)
        "comma" => 0xBC,
        "period" => 0xBE,
        "slash" => 0xBF,
        "semicolon" => 0xBA,
        "quote" => 0xDE,
        "left_bracket" => 0xDB,
        "right_bracket" => 0xDD,
        "backslash" => 0xDC,
        "grave" => 0xC0,
        "minus" => 0xBD,
        "equal" => 0xBB,

        // Modifiers. Semantic translation: command -> Ctrl, option -> Alt.
        "shift" => 0x10,
        "right_shift" => 0xA1,
        "control" | "ctrl" | "command" => 0x11,
        "right_control" | "right_command" => 0xA3,
        "option" | "alt" => 0x12,
        "right_option" | "right_alt" => 0xA5,
        "win" | "meta" => 0x5B,

        _ => return None,
    };
    Some(vk)
}

/// Whether a VK reported by the low-level hook is a modifier key.
pub fn is_modifier_vk(vk: u16) -> bool {
    matches!(
        vk,
        0x10 | 0x11 | 0x12          // generic Shift / Ctrl / Alt
        | 0xA0..=0xA5               // L/R Shift, Ctrl, Alt
        | 0x5B | 0x5C               // L/R Win
        | 0x14                      // Caps Lock (mac version also treats it as one)
    )
}

/// Keys that require KEYEVENTF_EXTENDEDKEY when injected.
pub fn is_extended(vk: u16) -> bool {
    matches!(
        vk,
        0x21..=0x28                 // PgUp PgDn End Home arrows
        | 0x2D | 0x2E               // Insert, Delete
        | 0xA3 | 0xA5               // Right Ctrl, Right Alt
        | 0x5B | 0x5C               // Win keys
    )
}
