use global_hotkey::hotkey::{Code, HotKey, Modifiers as HKModifiers};
use std::{collections::HashMap, sync::LazyLock};

use iced::keyboard::{Key, Modifiers, key::Named};

static NAMED_KEY: LazyLock<HashMap<&'static str, Named>> = LazyLock::new(|| {
    let mut map = HashMap::new();
    macro_rules! add_to_map {
        ($($k:ident),+ ; $($ignored:ident),*) => {{
            #[deny(unreachable_patterns)]
            const _: () = match Named::Alt { $(Named::$k)|* | $(Named::$ignored)|* => () };

            $(map.insert(stringify!($k), Named::$k);)*
        }};
    }
    #[rustfmt::skip]
    add_to_map!(AltGraph, CapsLock, Fn, FnLock, NumLock, ScrollLock, Symbol, SymbolLock, Enter, Tab, Space, ArrowDown, ArrowLeft, ArrowRight, ArrowUp, End, Home, PageDown, PageUp, Backspace, Clear, Copy, CrSel, Cut, Delete, EraseEof, ExSel, Insert, Paste, Redo, Undo, Accept, Again, Attn, Cancel, ContextMenu, Execute, Find, Help, Pause, Play, Props, Select, ZoomIn, ZoomOut, BrightnessDown, BrightnessUp, Eject, LogOff, Power, PowerOff, PrintScreen, Hibernate, Standby, WakeUp, AllCandidates, Alphanumeric, CodeInput, Compose, Convert, FinalMode, GroupFirst, GroupLast, GroupNext, GroupPrevious, ModeChange, NextCandidate, NonConvert, PreviousCandidate, Process, SingleCandidate, HangulMode, HanjaMode, JunjaMode, Eisu, Hankaku, Hiragana, HiraganaKatakana, KanaMode, KanjiMode, Katakana, Romaji, Zenkaku, ZenkakuHankaku, Soft1, Soft2, Soft3, Soft4, ChannelDown, ChannelUp, Close, MailForward, MailReply, MailSend, MediaClose, MediaFastForward, MediaPause, MediaPlay, MediaPlayPause, MediaRecord, MediaRewind, MediaStop, MediaTrackNext, MediaTrackPrevious, New, Open, Print, Save, SpellCheck, Key11, Key12, AudioBalanceLeft, AudioBalanceRight, AudioBassBoostDown, AudioBassBoostToggle, AudioBassBoostUp, AudioFaderFront, AudioFaderRear, AudioSurroundModeNext, AudioTrebleDown, AudioTrebleUp, AudioVolumeDown, AudioVolumeUp, AudioVolumeMute, MicrophoneToggle, MicrophoneVolumeDown, MicrophoneVolumeUp, MicrophoneVolumeMute, SpeechCorrectionList, SpeechInputToggle, LaunchApplication1, LaunchApplication2, LaunchCalendar, LaunchContacts, LaunchMail, LaunchMediaPlayer, LaunchMusicPlayer, LaunchPhone, LaunchScreenSaver, LaunchSpreadsheet, LaunchWebBrowser, LaunchWebCam, LaunchWordProcessor, BrowserBack, BrowserFavorites, BrowserForward, BrowserHome, BrowserRefresh, BrowserSearch, BrowserStop, AppSwitch, Call, Camera, CameraFocus, EndCall, GoBack, GoHome, HeadsetHook, LastNumberRedial, Notification, MannerMode, VoiceDial, TV, TV3DMode, TVAntennaCable, TVAudioDescription, TVAudioDescriptionMixDown, TVAudioDescriptionMixUp, TVContentsMenu, TVDataService, TVInput, TVInputComponent1, TVInputComponent2, TVInputComposite1, TVInputComposite2, TVInputHDMI1, TVInputHDMI2, TVInputHDMI3, TVInputHDMI4, TVInputVGA1, TVMediaContext, TVNetwork, TVNumberEntry, TVPower, TVRadioService, TVSatellite, TVSatelliteBS, TVSatelliteCS, TVSatelliteToggle, TVTerrestrialAnalog, TVTerrestrialDigital, TVTimer, AVRInput, AVRPower, ColorF0Red, ColorF1Green, ColorF2Yellow, ColorF3Blue, ColorF4Grey, ColorF5Brown, ClosedCaptionToggle, Dimmer, DisplaySwap, DVR, Exit, FavoriteClear0, FavoriteClear1, FavoriteClear2, FavoriteClear3, FavoriteRecall0, FavoriteRecall1, FavoriteRecall2, FavoriteRecall3, FavoriteStore0, FavoriteStore1, FavoriteStore2, FavoriteStore3, Guide, GuideNextDay, GuidePreviousDay, Info, InstantReplay, Link, ListProgram, LiveContent, Lock, MediaApps, MediaAudioTrack, MediaLast, MediaSkipBackward, MediaSkipForward, MediaStepBackward, MediaStepForward, MediaTopMenu, NavigateIn, NavigateNext, NavigateOut, NavigatePrevious, NextFavoriteChannel, NextUserProfile, OnDemand, Pairing, PinPDown, PinPMove, PinPToggle, PinPUp, PlaySpeedDown, PlaySpeedReset, PlaySpeedUp, RandomToggle, RcLowBattery, RecordSpeedNext, RfBypass, ScanChannelsToggle, ScreenModeNext, Settings, SplitScreenToggle, STBInput, STBPower, Subtitle, Teletext, VideoModeNext, Wink, ZoomToggle, F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12, F13, F14, F15, F16, F17, F18, F19, F20, F21, F22, F23, F24, F25, F26, F27, F28, F29, F30, F31, F32, F33, F34, F35; Alt, Control, Super, Hyper, Meta, Shift, Escape);
    map
});

pub fn key_from_str(s: &str) -> Option<Key> {
    let s = s.trim().to_lowercase();
    NAMED_KEY
        .get(&s as &str)
        .copied()
        .map(Key::Named)
        .or_else(|| Some(Key::Character(s.try_into().ok()?)))
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
            return Some((modifiers, key_from_str(next)?));
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
