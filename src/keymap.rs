use rdev::Key;

/// Parse une combinaison de touches "Ctrl+Alt+F10" → ensemble de CanonKey.
/// Les modificateurs sont normalisés (CtrlL représente Ctrl gauche ou droit).
pub fn parse_emergency_combo(s: &str) -> anyhow::Result<Vec<CanonKey>> {
    let mut out: Vec<CanonKey> = Vec::new();
    for tok in s.split('+') {
        let t = tok.trim();
        let k = match t.to_ascii_lowercase().as_str() {
            "ctrl" | "control" | "ctrlleft" | "ctrlright" => CanonKey::CtrlL,
            "alt" | "altleft" | "altright" => CanonKey::AltL,
            "shift" | "shiftleft" | "shiftright" => CanonKey::ShiftL,
            "meta" | "win" | "super" => CanonKey::MetaL,
            "space" | " " => CanonKey::Space,
            "enter" => CanonKey::Enter,
            "tab" => CanonKey::Tab,
            "escape" | "esc" => CanonKey::Escape,
            v if v.starts_with('f') && v.len() > 1 => {
                let n: u16 = v[1..].parse()?;
                if !(1..=12).contains(&n) {
                    anyhow::bail!("touche fonction {t} non supportée");
                }
                CanonKey::from_code(49 + n)
            }
            v if v.len() == 1 => {
                let c = v.chars().next().unwrap();
                if c.is_ascii_alphabetic() {
                    let idx = (c.to_ascii_uppercase() as u8 - b'A') as u16;
                    CanonKey::from_code(idx + 1)
                } else if c.is_ascii_digit() {
                    let n = c.to_digit(10).unwrap() as u16;
                    CanonKey::from_code(if n == 0 { 27 } else { 27 + n })
                } else {
                    anyhow::bail!("touche inconnue dans la combinaison: {t}");
                }
            }
            _ => anyhow::bail!("touche inconnue dans la combinaison: {t}"),
        };
        out.push(k);
    }
    if out.is_empty() {
        anyhow::bail!("combinaison d'urgence vide");
    }
    Ok(out)
}

#[cfg(test)]
mod combo_tests {
    use super::*;

    #[test]
    fn combo_parsee() {
        let c = parse_emergency_combo("Ctrl+Alt+F10").unwrap();
        assert_eq!(c, vec![CanonKey::CtrlL, CanonKey::AltL, CanonKey::F10]);
    }

    #[test]
    fn lettres_et_chiffres() {
        assert_eq!(parse_emergency_combo("Shift+Z").unwrap(), vec![CanonKey::ShiftL, CanonKey::KeyZ]);
        assert_eq!(parse_emergency_combo("Alt+9").unwrap(), vec![CanonKey::AltL, CanonKey::D9]);
        assert!(parse_emergency_combo("Key0").is_err());
    }

    #[test]
    fn inconnue_rejetee() {
        assert!(parse_emergency_combo("Ctrl+Brouzouf").is_err());
        assert!(parse_emergency_combo("").is_err());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CanonKey {
    KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ, KeyK, KeyL, KeyM,
    KeyN, KeyO, KeyP, KeyQ, KeyR, KeyS, KeyT, KeyU, KeyV, KeyW, KeyX, KeyY, KeyZ,
    D0, D1, D2, D3, D4, D5, D6, D7, D8, D9,
    Enter, Tab, Space, Backspace, Escape,
    ShiftL, ShiftR, CtrlL, CtrlR, AltL, AltR, MetaL, MetaR,
    F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
    Home, End, PageUp, PageDown, Insert, Delete,
    ArrowUp, ArrowDown, ArrowLeft, ArrowRight,
    Comma, Period, Slash, Backslash, Semicolon, Quote, BracketL, BracketR, Minus, Equal,
    CapsLock, NumLock, ScrollLock, PrintScreen, Pause,
    Other { code: u16 },
}

impl CanonKey {
    pub fn code(&self) -> u16 {
        match self {
            CanonKey::KeyA => 1, CanonKey::KeyB => 2, CanonKey::KeyC => 3, CanonKey::KeyD => 4,
            CanonKey::KeyE => 5, CanonKey::KeyF => 6, CanonKey::KeyG => 7, CanonKey::KeyH => 8,
            CanonKey::KeyI => 9, CanonKey::KeyJ => 10, CanonKey::KeyK => 11, CanonKey::KeyL => 12,
            CanonKey::KeyM => 13, CanonKey::KeyN => 14, CanonKey::KeyO => 15, CanonKey::KeyP => 16,
            CanonKey::KeyQ => 17, CanonKey::KeyR => 18, CanonKey::KeyS => 19, CanonKey::KeyT => 20,
            CanonKey::KeyU => 21, CanonKey::KeyV => 22, CanonKey::KeyW => 23, CanonKey::KeyX => 24,
            CanonKey::KeyY => 25, CanonKey::KeyZ => 26,
            CanonKey::D0 => 27, CanonKey::D1 => 28, CanonKey::D2 => 29, CanonKey::D3 => 30,
            CanonKey::D4 => 31, CanonKey::D5 => 32, CanonKey::D6 => 33, CanonKey::D7 => 34,
            CanonKey::D8 => 35, CanonKey::D9 => 36,
            CanonKey::Enter => 37, CanonKey::Tab => 38, CanonKey::Space => 39,
            CanonKey::Backspace => 40, CanonKey::Escape => 41,
            CanonKey::ShiftL => 42, CanonKey::ShiftR => 43, CanonKey::CtrlL => 44,
            CanonKey::CtrlR => 45, CanonKey::AltL => 46, CanonKey::AltR => 47,
            CanonKey::MetaL => 48, CanonKey::MetaR => 49,
            CanonKey::F1 => 50, CanonKey::F2 => 51, CanonKey::F3 => 52, CanonKey::F4 => 53,
            CanonKey::F5 => 54, CanonKey::F6 => 55, CanonKey::F7 => 56, CanonKey::F8 => 57,
            CanonKey::F9 => 58, CanonKey::F10 => 59, CanonKey::F11 => 60, CanonKey::F12 => 61,
            CanonKey::Home => 62, CanonKey::End => 63, CanonKey::PageUp => 64, CanonKey::PageDown => 65,
            CanonKey::Insert => 66, CanonKey::Delete => 67,
            CanonKey::ArrowUp => 68, CanonKey::ArrowDown => 69, CanonKey::ArrowLeft => 70, CanonKey::ArrowRight => 71,
            CanonKey::Comma => 72, CanonKey::Period => 73, CanonKey::Slash => 74, CanonKey::Backslash => 75,
            CanonKey::Semicolon => 76, CanonKey::Quote => 77, CanonKey::BracketL => 78, CanonKey::BracketR => 79,
            CanonKey::Minus => 80, CanonKey::Equal => 81,
            CanonKey::CapsLock => 82, CanonKey::NumLock => 83, CanonKey::ScrollLock => 84,
            CanonKey::PrintScreen => 85, CanonKey::Pause => 86,
            CanonKey::Other { code } => *code,
        }
    }

    pub fn from_code(code: u16) -> CanonKey {
        for c in ALL_CANON {
            if c.code() == code {
                return *c;
            }
        }
        CanonKey::Other { code }
    }
}

const ALL_CANON: &[CanonKey] = &[
    CanonKey::KeyA, CanonKey::KeyB, CanonKey::KeyC, CanonKey::KeyD, CanonKey::KeyE,
    CanonKey::KeyF, CanonKey::KeyG, CanonKey::KeyH, CanonKey::KeyI, CanonKey::KeyJ,
    CanonKey::KeyK, CanonKey::KeyL, CanonKey::KeyM, CanonKey::KeyN, CanonKey::KeyO,
    CanonKey::KeyP, CanonKey::KeyQ, CanonKey::KeyR, CanonKey::KeyS, CanonKey::KeyT,
    CanonKey::KeyU, CanonKey::KeyV, CanonKey::KeyW, CanonKey::KeyX, CanonKey::KeyY,
    CanonKey::KeyZ, CanonKey::D0, CanonKey::D1, CanonKey::D2, CanonKey::D3, CanonKey::D4,
    CanonKey::D5, CanonKey::D6, CanonKey::D7, CanonKey::D8, CanonKey::D9, CanonKey::Enter,
    CanonKey::Tab, CanonKey::Space, CanonKey::Backspace, CanonKey::Escape,
    CanonKey::ShiftL, CanonKey::ShiftR, CanonKey::CtrlL, CanonKey::CtrlR,
    CanonKey::AltL, CanonKey::AltR, CanonKey::MetaL, CanonKey::MetaR,
    CanonKey::F1, CanonKey::F2, CanonKey::F3, CanonKey::F4, CanonKey::F5, CanonKey::F6,
    CanonKey::F7, CanonKey::F8, CanonKey::F9, CanonKey::F10, CanonKey::F11, CanonKey::F12,
    CanonKey::Home, CanonKey::End, CanonKey::PageUp, CanonKey::PageDown,
    CanonKey::Insert, CanonKey::Delete, CanonKey::ArrowUp, CanonKey::ArrowDown,
    CanonKey::ArrowLeft, CanonKey::ArrowRight, CanonKey::Comma, CanonKey::Period,
    CanonKey::Slash, CanonKey::Backslash, CanonKey::Semicolon, CanonKey::Quote,
    CanonKey::BracketL, CanonKey::BracketR, CanonKey::Minus, CanonKey::Equal,
    CanonKey::CapsLock, CanonKey::NumLock, CanonKey::ScrollLock,
    CanonKey::PrintScreen, CanonKey::Pause,
];

pub fn rdev_key_to_canon(k: &Key) -> CanonKey {
    use rdev::Key as K;
    match k {
        K::KeyA => CanonKey::KeyA, K::KeyB => CanonKey::KeyB, K::KeyC => CanonKey::KeyC,
        K::KeyD => CanonKey::KeyD, K::KeyE => CanonKey::KeyE, K::KeyF => CanonKey::KeyF,
        K::KeyG => CanonKey::KeyG, K::KeyH => CanonKey::KeyH, K::KeyI => CanonKey::KeyI,
        K::KeyJ => CanonKey::KeyJ, K::KeyK => CanonKey::KeyK, K::KeyL => CanonKey::KeyL,
        K::KeyM => CanonKey::KeyM, K::KeyN => CanonKey::KeyN, K::KeyO => CanonKey::KeyO,
        K::KeyP => CanonKey::KeyP, K::KeyQ => CanonKey::KeyQ, K::KeyR => CanonKey::KeyR,
        K::KeyS => CanonKey::KeyS, K::KeyT => CanonKey::KeyT, K::KeyU => CanonKey::KeyU,
        K::KeyV => CanonKey::KeyV, K::KeyW => CanonKey::KeyW, K::KeyX => CanonKey::KeyX,
        K::KeyY => CanonKey::KeyY, K::KeyZ => CanonKey::KeyZ,
        K::Num0 => CanonKey::D0, K::Num1 => CanonKey::D1, K::Num2 => CanonKey::D2,
        K::Num3 => CanonKey::D3, K::Num4 => CanonKey::D4, K::Num5 => CanonKey::D5,
        K::Num6 => CanonKey::D6, K::Num7 => CanonKey::D7, K::Num8 => CanonKey::D8,
        K::Num9 => CanonKey::D9,
        K::Return => CanonKey::Enter, K::Tab => CanonKey::Tab, K::Space => CanonKey::Space,
        K::Backspace => CanonKey::Backspace, K::Escape => CanonKey::Escape,
        K::ShiftLeft => CanonKey::ShiftL, K::ShiftRight => CanonKey::ShiftR,
        K::ControlLeft => CanonKey::CtrlL, K::ControlRight => CanonKey::CtrlR,
        K::Alt => CanonKey::AltL, K::AltGr => CanonKey::AltR,
        K::MetaLeft => CanonKey::MetaL, K::MetaRight => CanonKey::MetaR,
        K::F1 => CanonKey::F1, K::F2 => CanonKey::F2, K::F3 => CanonKey::F3,
        K::F4 => CanonKey::F4, K::F5 => CanonKey::F5, K::F6 => CanonKey::F6,
        K::F7 => CanonKey::F7, K::F8 => CanonKey::F8, K::F9 => CanonKey::F9,
        K::F10 => CanonKey::F10, K::F11 => CanonKey::F11, K::F12 => CanonKey::F12,
        K::Home => CanonKey::Home, K::End => CanonKey::End,
        K::PageUp => CanonKey::PageUp, K::PageDown => CanonKey::PageDown,
        K::Insert => CanonKey::Insert, K::Delete => CanonKey::Delete,
        K::UpArrow => CanonKey::ArrowUp, K::DownArrow => CanonKey::ArrowDown,
        K::LeftArrow => CanonKey::ArrowLeft, K::RightArrow => CanonKey::ArrowRight,
        K::Comma => CanonKey::Comma, K::Dot => CanonKey::Period,
        K::Slash => CanonKey::Slash, K::BackSlash => CanonKey::Backslash,
        K::SemiColon => CanonKey::Semicolon, K::Quote => CanonKey::Quote,
        K::LeftBracket => CanonKey::BracketL, K::RightBracket => CanonKey::BracketR,
        K::Minus => CanonKey::Minus, K::Equal => CanonKey::Equal,
        K::CapsLock => CanonKey::CapsLock, K::NumLock => CanonKey::NumLock,
        K::ScrollLock => CanonKey::ScrollLock, K::PrintScreen => CanonKey::PrintScreen,
        K::Pause => CanonKey::Pause,
        K::BackQuote => CanonKey::Other { code: CanonKey::Equal.code() + 100 },
        K::IntlBackslash => CanonKey::Other { code: CanonKey::Equal.code() + 101 },
        _ => {
            tracing::warn!("touche hors table: {k:?}");
            CanonKey::Other { code: u16::MAX }
        }
    }
}

pub fn canon_to_rdev(c: CanonKey) -> Key {
    use rdev::Key as K;
    match c {
        CanonKey::KeyA => K::KeyA, CanonKey::KeyB => K::KeyB, CanonKey::KeyC => K::KeyC,
        CanonKey::KeyD => K::KeyD, CanonKey::KeyE => K::KeyE, CanonKey::KeyF => K::KeyF,
        CanonKey::KeyG => K::KeyG, CanonKey::KeyH => K::KeyH, CanonKey::KeyI => K::KeyI,
        CanonKey::KeyJ => K::KeyJ, CanonKey::KeyK => K::KeyK, CanonKey::KeyL => K::KeyL,
        CanonKey::KeyM => K::KeyM, CanonKey::KeyN => K::KeyN, CanonKey::KeyO => K::KeyO,
        CanonKey::KeyP => K::KeyP, CanonKey::KeyQ => K::KeyQ, CanonKey::KeyR => K::KeyR,
        CanonKey::KeyS => K::KeyS, CanonKey::KeyT => K::KeyT, CanonKey::KeyU => K::KeyU,
        CanonKey::KeyV => K::KeyV, CanonKey::KeyW => K::KeyW, CanonKey::KeyX => K::KeyX,
        CanonKey::KeyY => K::KeyY, CanonKey::KeyZ => K::KeyZ,
        CanonKey::D0 => K::Num0, CanonKey::D1 => K::Num1, CanonKey::D2 => K::Num2,
        CanonKey::D3 => K::Num3, CanonKey::D4 => K::Num4, CanonKey::D5 => K::Num5,
        CanonKey::D6 => K::Num6, CanonKey::D7 => K::Num7, CanonKey::D8 => K::Num8,
        CanonKey::D9 => K::Num9,
        CanonKey::Enter => K::Return, CanonKey::Tab => K::Tab, CanonKey::Space => K::Space,
        CanonKey::Backspace => K::Backspace, CanonKey::Escape => K::Escape,
        CanonKey::ShiftL => K::ShiftLeft, CanonKey::ShiftR => K::ShiftRight,
        CanonKey::CtrlL => K::ControlLeft, CanonKey::CtrlR => K::ControlRight,
        CanonKey::AltL => K::Alt, CanonKey::AltR => K::AltGr,
        CanonKey::MetaL => K::MetaLeft, CanonKey::MetaR => K::MetaRight,
        CanonKey::F1 => K::F1, CanonKey::F2 => K::F2, CanonKey::F3 => K::F3,
        CanonKey::F4 => K::F4, CanonKey::F5 => K::F5, CanonKey::F6 => K::F6,
        CanonKey::F7 => K::F7, CanonKey::F8 => K::F8, CanonKey::F9 => K::F9,
        CanonKey::F10 => K::F10, CanonKey::F11 => K::F11, CanonKey::F12 => K::F12,
        CanonKey::Home => K::Home, CanonKey::End => K::End,
        CanonKey::PageUp => K::PageUp, CanonKey::PageDown => K::PageDown,
        CanonKey::Insert => K::Insert, CanonKey::Delete => K::Delete,
        CanonKey::ArrowUp => K::UpArrow, CanonKey::ArrowDown => K::DownArrow,
        CanonKey::ArrowLeft => K::LeftArrow, CanonKey::ArrowRight => K::RightArrow,
        CanonKey::Comma => K::Comma, CanonKey::Period => K::Dot,
        CanonKey::Slash => K::Slash, CanonKey::Backslash => K::BackSlash,
        CanonKey::Semicolon => K::SemiColon, CanonKey::Quote => K::Quote,
        CanonKey::BracketL => K::LeftBracket, CanonKey::BracketR => K::RightBracket,
        CanonKey::Minus => K::Minus, CanonKey::Equal => K::Equal,
        CanonKey::CapsLock => K::CapsLock, CanonKey::NumLock => K::NumLock,
        CanonKey::ScrollLock => K::ScrollLock, CanonKey::PrintScreen => K::PrintScreen,
        CanonKey::Pause => K::Pause,
        CanonKey::Other { .. } => {
            tracing::warn!("canon hors table injectée telle quelle (non supportée)");
            K::Escape // fallback sûr — pas d'action involontaire
        }
    }
}

pub fn button_to_id(b: &rdev::Button) -> u8 {
    match b {
        rdev::Button::Left => 1,
        rdev::Button::Right => 2,
        rdev::Button::Middle => 3,
        rdev::Button::Unknown(n) => *n,
    }
}

pub fn id_to_button(id: u8) -> Option<rdev::Button> {
    match id {
        1 => Some(rdev::Button::Left),
        2 => Some(rdev::Button::Right),
        3 => Some(rdev::Button::Middle),
        0 => None, // sentinel for unknown
        n => Some(rdev::Button::Unknown(n)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapping_canonique_retour() {
        for k in [
            rdev::Key::KeyA,
            rdev::Key::KeyZ,
            rdev::Key::Num1,
            rdev::Key::F5,
            rdev::Key::LeftArrow,
            rdev::Key::Return,
            rdev::Key::ShiftLeft,
        ] {
            let c = rdev_key_to_canon(&k);
            assert_eq!(canon_to_rdev(c), k, "roundtrip failed for {k:?}");
        }
        assert_eq!(
            CanonKey::KeyA.code(),
            CanonKey::from_code(CanonKey::KeyA.code()).code()
        );
        assert_eq!(
            rdev::Button::Left,
            id_to_button(button_to_id(&rdev::Button::Left)).unwrap()
        );
        assert_eq!(
            rdev::Button::Right,
            id_to_button(button_to_id(&rdev::Button::Right)).unwrap()
        );
        assert_eq!(
            rdev::Button::Middle,
            id_to_button(button_to_id(&rdev::Button::Middle)).unwrap()
        );
    }
}