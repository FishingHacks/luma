use std::{borrow::Cow, iter};

use iced::clipboard;

use crate::{CustomData, Entry, Message, Plugin, ResultBuilder, matcher::MatcherInput};

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

    async fn get_for_values(&self, input: &MatcherInput<'_>, builder: &ResultBuilder) {
        let words = input.words();
        // <value> <unit> to <unit>
        if words.len() != 4 || words[2] != "to" {
            return;
        }
        let Ok(amount) = words[0].parse::<f64>() else {
            return;
        };
        for conversion in CONVERSIONS {
            if conversion.0.eq_ignore_ascii_case(words[1])
                && conversion.1.eq_ignore_ascii_case(words[3])
            {
                let result = amount * conversion.2;
                return builder
                    .commit(iter::once(Entry {
                        name: format!("{} {}", result, conversion.1),
                        subtitle: Cow::Owned(format!("Converted from {} {}", amount, conversion.0)),
                        plugin: self.prefix(),
                        data: CustomData::new((result, conversion.1)),
                    }))
                    .await;
            }
        }
    }

    fn init(&mut self) {}

    fn handle(&self, thing: CustomData) -> iced::Task<Message> {
        let result = thing.into::<(f64, &'static str)>();
        let value = if result.0 == result.0.floor() {
            format!("{} {}", result.0 as i64, result.1)
        } else {
            format!("{} {}", result.0, result.1)
        };
        println!("Writing {value}");
        clipboard::write(value)
    }
}
