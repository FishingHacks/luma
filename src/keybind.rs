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
