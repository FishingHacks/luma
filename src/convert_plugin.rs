use iced::{Task, clipboard};

use crate::{Action, CustomData, Entry, Message, Plugin, ResultBuilderRef, matcher::MatcherInput};

// Currency: See https://www.exchangerate-api.com/docs/free

static CONVERSIONS: &[(&str, &str, f64)] = &[
    ("ml", "l", 0.001),
    ("l", "ml", 1000.0),
    ("mg", "g", 0.001),
    ("g", "mg", 1000.0),
];

#[derive(Default)]
pub struct ConvertPlugin;

impl Plugin for ConvertPlugin {
    #[inline(always)]
    fn prefix(&self) -> &'static str {
        "convert"
    }

    async fn get_for_values(&self, input: &MatcherInput, builder: ResultBuilderRef<'_>) {
        let mut words = input.input().split(" ");
        // <value> <unit> to <unit>
        let Some(value) = words.next() else { return };
        let Some(unit_from) = words.next() else {
            return;
        };
        let Some(to) = words.next() else { return };
        let Some(unit_to) = words.next() else { return };
        if words.next().is_some() || to != "to" {
            return;
        }
        let Ok(amount) = value.parse::<f64>() else {
            return;
        };
        for conversion in CONVERSIONS {
            if conversion.0.eq_ignore_ascii_case(unit_from)
                && conversion.1.eq_ignore_ascii_case(unit_to)
            {
                let result = amount * conversion.2;
                let name = format!("{} {}", result, conversion.1);
                let subtitle = format!("Converted from {} {}", amount, conversion.0);
                return builder
                    .add(Entry::new(name, subtitle, CustomData::new((result, conversion.1))).pin())
                    .await;
            }
        }
    }

    fn init(&mut self) {}

    fn handle_pre(&self, thing: CustomData, action: &str) -> iced::Task<Message> {
        if action == "copy" {
            let result = thing.into::<(f64, &'static str)>();
            let value = if result.0 == result.0.floor() {
                format!("{} {}", result.0 as i64, result.1)
            } else {
                format!("{} {}", result.0, result.1)
            };
            clipboard::write(value)
        } else {
            let result = thing.into::<(f64, &'static str)>();
            let value = if result.0 == result.0.floor() {
                format!("convert {} {} to", result.0 as i64, result.1)
            } else {
                format!("convert {} {} to", result.0, result.1)
            };
            Task::done(Message::SetSearch(value))
        }
    }

    fn actions(&self) -> &'static [crate::Action] {
        const {
            &[
                Action::default("Copy to clipboard", "copy"),
                Action::suggest("Convert to", "suggest").keep_open(),
            ]
        }
    }
}
