use global_hotkey::hotkey::{Code, HotKey, Modifiers as HKModifiers};
use std::{collections::HashMap, sync::LazyLock};

use iced::keyboard::{Key, Modifiers, key::Named};

static NAMED_KEY: LazyLock<HashMap<&'static str, Named>> = LazyLock::new(|| {
    let mut map = HashMap::new();
    map.insert("alt", Named::Alt);
    map.insert("altgr", Named::AltGraph);
    map.insert("capslock", Named::CapsLock);
    map.insert("control", Named::Control);
    map.insert("ctrl", Named::Control);
    map.insert("fn", Named::Fn);
    map.insert("fnlock", Named::FnLock);
    map.insert("numlock", Named::NumLock);
    map.insert("scrllck", Named::ScrollLock);
    map.insert("scrolllock", Named::ScrollLock);
    map.insert("shift", Named::Shift);
    map.insert("symbol", Named::Symbol);
    map.insert("symbollock", Named::SymbolLock);
    map.insert("super", Named::Super);
    map.insert("meta", Named::Super);
    map.insert("win", Named::Super);
    map.insert("cmd", Named::Super);
    map.insert("command", Named::Super);
    map.insert("enter", Named::Enter);
    map.insert("tab", Named::Tab);
    map.insert("space", Named::Space);
    map.insert("down", Named::ArrowDown);
    map.insert("left", Named::ArrowLeft);
    map.insert("right", Named::ArrowRight);
    map.insert("up", Named::ArrowUp);
    map.insert("end", Named::End);
    map.insert("home", Named::Home);
    map.insert("pgdn", Named::PageDown);
    map.insert("pgdwn", Named::PageDown);
    map.insert("pagedown", Named::PageDown);
    map.insert("pgup", Named::PageUp);
    map.insert("pageup", Named::PageUp);
    map.insert("backspace", Named::Backspace);
    map.insert("clear", Named::Clear);
    map.insert("copy", Named::Copy);
    map.insert("cut", Named::Cut);
    map.insert("del", Named::Delete);
    map.insert("delete", Named::Delete);
    map.insert("insert", Named::Insert);
    map.insert("paste", Named::Paste);
    map.insert("redo", Named::Redo);
    map.insert("undo", Named::Undo);
    map.insert("accept", Named::Accept);
    map.insert("again", Named::Again);
    map.insert("pause", Named::Pause);
    map.insert("play", Named::Play);
    map.insert("select", Named::Select);
    map.insert("new", Named::New);
    map.insert("open", Named::Open);
    map.insert("print", Named::Print);
    map.insert("save", Named::Save);
    map.insert("f1", Named::F1);
    map.insert("f2", Named::F2);
    map.insert("f3", Named::F3);
    map.insert("f4", Named::F4);
    map.insert("f5", Named::F5);
    map.insert("f6", Named::F6);
    map.insert("f7", Named::F7);
    map.insert("f8", Named::F8);
    map.insert("f9", Named::F9);
    map.insert("f10", Named::F10);
    map.insert("f11", Named::F11);
    map.insert("f12", Named::F12);
    map.insert("f13", Named::F13);
    map.insert("f14", Named::F14);
    map.insert("f15", Named::F15);
    map.insert("f16", Named::F16);
    map.insert("f17", Named::F17);
    map.insert("f18", Named::F18);
    map.insert("f19", Named::F19);
    map.insert("f20", Named::F20);
    map.insert("f21", Named::F21);
    map.insert("f22", Named::F22);
    map.insert("f23", Named::F23);
    map.insert("f24", Named::F24);
    map.insert("f25", Named::F25);
    map.insert("f26", Named::F26);
    map.insert("f27", Named::F27);
    map.insert("f28", Named::F28);
    map.insert("f29", Named::F29);
    map.insert("f30", Named::F30);
    map.insert("f31", Named::F31);
    map.insert("f32", Named::F32);
    map.insert("f33", Named::F33);
    map.insert("f34", Named::F34);
    map.insert("f35", Named::F35);
    map
});

pub fn key_from_str(s: &str) -> Key {
    let s = s.trim().to_lowercase();
    NAMED_KEY
        .get(&s as &str)
        .copied()
        .map_or_else(move || Key::Character(s.into()), Key::Named)
}

pub fn modifier_from_str(s: &str) -> Option<Modifiers> {
    let s = s.trim();
    if s.eq_ignore_ascii_case("ctrl") {
        Some(Modifiers::CTRL)
    } else if s.eq_ignore_ascii_case("alt") {
        Some(Modifiers::ALT)
    } else if s.eq_ignore_ascii_case("shift") {
        Some(Modifiers::SHIFT)
    } else if s.eq_ignore_ascii_case("win")
        || s.eq_ignore_ascii_case("cmd")
        || s.eq_ignore_ascii_case("super")
        || s.eq_ignore_ascii_case("meta")
    {
        Some(Modifiers::LOGO)
    } else {
        None
    }
}

pub fn key_and_modifiers_from_str(s: &str) -> Option<(Modifiers, Key)> {
    if s.is_empty() {
        return None;
    }
    let mut peekable = s.split('+').peekable();
    let mut modifiers = Modifiers::empty();
    loop {
        let next = peekable.next()?.trim();
        if peekable.peek().is_none() {
            return Some((modifiers, key_from_str(next)));
        }
        modifiers |= modifier_from_str(next)?;
    }
}

pub fn iced_key_to_code(key: Key) -> Option<Code> {
    match key {
        Key::Named(Named::CapsLock) => Some(Code::CapsLock),
        Key::Named(Named::Fn) => Some(Code::Fn),
        Key::Named(Named::FnLock) => Some(Code::FnLock),
        Key::Named(Named::NumLock) => Some(Code::NumLock),
        Key::Named(Named::ScrollLock) => Some(Code::ScrollLock),
        Key::Named(Named::Enter) => Some(Code::Enter),
        Key::Named(Named::Tab) => Some(Code::Tab),
        Key::Named(Named::Space) => Some(Code::Space),
        Key::Named(Named::ArrowDown) => Some(Code::ArrowDown),
        Key::Named(Named::ArrowLeft) => Some(Code::ArrowLeft),
        Key::Named(Named::ArrowRight) => Some(Code::ArrowRight),
        Key::Named(Named::ArrowUp) => Some(Code::ArrowUp),
        Key::Named(Named::End) => Some(Code::End),
        Key::Named(Named::Home) => Some(Code::Home),
        Key::Named(Named::PageDown) => Some(Code::PageDown),
        Key::Named(Named::PageUp) => Some(Code::PageUp),
        Key::Named(Named::Backspace) => Some(Code::Backspace),
        Key::Named(Named::Copy) => Some(Code::Copy),
        Key::Named(Named::Cut) => Some(Code::Cut),
        Key::Named(Named::Delete) => Some(Code::Delete),
        Key::Named(Named::Insert) => Some(Code::Insert),
        Key::Named(Named::Paste) => Some(Code::Paste),
        Key::Named(Named::Undo) => Some(Code::Undo),
        Key::Named(Named::Again) => Some(Code::Again),
        Key::Named(Named::Pause) => Some(Code::Pause),
        Key::Named(Named::Select) => Some(Code::Select),
        Key::Named(Named::Open) => Some(Code::Open),
        Key::Named(Named::F1) => Some(Code::F1),
        Key::Named(Named::F2) => Some(Code::F2),
        Key::Named(Named::F3) => Some(Code::F3),
        Key::Named(Named::F4) => Some(Code::F4),
        Key::Named(Named::F5) => Some(Code::F5),
        Key::Named(Named::F6) => Some(Code::F6),
        Key::Named(Named::F7) => Some(Code::F7),
        Key::Named(Named::F8) => Some(Code::F8),
        Key::Named(Named::F9) => Some(Code::F9),
        Key::Named(Named::F10) => Some(Code::F10),
        Key::Named(Named::F11) => Some(Code::F11),
        Key::Named(Named::F12) => Some(Code::F12),
        Key::Named(Named::F13) => Some(Code::F13),
        Key::Named(Named::F14) => Some(Code::F14),
        Key::Named(Named::F15) => Some(Code::F15),
        Key::Named(Named::F16) => Some(Code::F16),
        Key::Named(Named::F17) => Some(Code::F17),
        Key::Named(Named::F18) => Some(Code::F18),
        Key::Named(Named::F19) => Some(Code::F19),
        Key::Named(Named::F20) => Some(Code::F20),
        Key::Named(Named::F21) => Some(Code::F21),
        Key::Named(Named::F22) => Some(Code::F22),
        Key::Named(Named::F23) => Some(Code::F23),
        Key::Named(Named::F24) => Some(Code::F24),
        Key::Named(Named::F25) => Some(Code::F25),
        Key::Named(Named::F26) => Some(Code::F26),
        Key::Named(Named::F27) => Some(Code::F27),
        Key::Named(Named::F28) => Some(Code::F28),
        Key::Named(Named::F29) => Some(Code::F29),
        Key::Named(Named::F30) => Some(Code::F30),
        Key::Named(Named::F31) => Some(Code::F31),
        Key::Named(Named::F32) => Some(Code::F32),
        Key::Named(Named::F33) => Some(Code::F33),
        Key::Named(Named::F34) => Some(Code::F34),
        Key::Named(Named::F35) => Some(Code::F35),
        Key::Character(c) => match c.as_str() {
            "`" => Some(Code::Backquote),
            "\\" => Some(Code::Backslash),
            "(" => Some(Code::BracketLeft),
            ")" => Some(Code::BracketRight),
            "," => Some(Code::Comma),
            "0" => Some(Code::Digit0),
            "1" => Some(Code::Digit1),
            "2" => Some(Code::Digit2),
            "3" => Some(Code::Digit3),
            "4" => Some(Code::Digit4),
            "5" => Some(Code::Digit5),
            "6" => Some(Code::Digit6),
            "7" => Some(Code::Digit7),
            "8" => Some(Code::Digit8),
            "9" => Some(Code::Digit9),
            "=" => Some(Code::Equal),
            "A" | "a" => Some(Code::KeyA),
            "B" | "b" => Some(Code::KeyB),
            "C" | "c" => Some(Code::KeyC),
            "D" | "d" => Some(Code::KeyD),
            "E" | "e" => Some(Code::KeyE),
            "F" | "f" => Some(Code::KeyF),
            "G" | "g" => Some(Code::KeyG),
            "H" | "h" => Some(Code::KeyH),
            "I" | "i" => Some(Code::KeyI),
            "J" | "j" => Some(Code::KeyJ),
            "K" | "k" => Some(Code::KeyK),
            "L" | "l" => Some(Code::KeyL),
            "M" | "m" => Some(Code::KeyM),
            "N" | "n" => Some(Code::KeyN),
            "O" | "o" => Some(Code::KeyO),
            "P" | "p" => Some(Code::KeyP),
            "Q" | "q" => Some(Code::KeyQ),
            "R" | "r" => Some(Code::KeyR),
            "S" | "s" => Some(Code::KeyS),
            "T" | "t" => Some(Code::KeyT),
            "U" | "u" => Some(Code::KeyU),
            "V" | "v" => Some(Code::KeyV),
            "W" | "w" => Some(Code::KeyW),
            "X" | "x" => Some(Code::KeyX),
            "Y" | "y" => Some(Code::KeyY),
            "Z" | "z" => Some(Code::KeyZ),
            "-" => Some(Code::Minus),
            "." => Some(Code::Period),
            "\"" => Some(Code::Quote),
            ";" => Some(Code::Semicolon),
            "/" => Some(Code::Slash),
            _ => None,
        },
        _ => None,
    }
}

pub fn iced_to_hotkey(keybind: (Modifiers, Key)) -> Option<HotKey> {
    let mut mods = HKModifiers::empty();
    if keybind.0.alt() {
        mods |= HKModifiers::ALT;
    }
    if keybind.0.control() {
        mods |= HKModifiers::CONTROL;
    }
    if keybind.0.shift() {
        mods |= HKModifiers::SHIFT;
    }
    if keybind.0.logo() {
        mods |= HKModifiers::SUPER;
    }
    Some(HotKey::new(Some(mods), iced_key_to_code(keybind.1)?))
}
