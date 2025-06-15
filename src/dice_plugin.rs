use iced::clipboard;
use rand::Rng;
use std::fmt::Write;

use crate::{Action, CustomData, Entry, Plugin, ResultBuilder, matcher::MatcherInput};

#[derive(Default)]
pub struct DicePlugin;

impl Plugin for DicePlugin {
    fn prefix(&self) -> &'static str {
        "roll"
    }

    async fn get_for_values(&self, input: &MatcherInput<'_>, builder: &ResultBuilder) {
        let words = input.words();
        if words.is_empty() {
            return;
        }
        let mut entries = Vec::with_capacity(words.len());
        let mut total = 0;
        for entry in words.iter().copied().filter_map(roll) {
            entries.push(entry.0);
            total += entry.1
        }
        if entries.len() > 1 {
            entries.insert(
                0,
                Entry {
                    name: format!("Overall Total:  {}", total),
                    subtitle: "".into(),
                    plugin: self.prefix(),
                    data: CustomData::new(total),
                },
            );
        }
        builder.commit(entries.into_iter()).await;
    }

    fn init(&mut self) {}

    fn handle(&self, thing: crate::CustomData, _: &str) -> iced::Task<crate::Message> {
        clipboard::write(format!("{}", thing.into::<usize>()))
    }

    fn actions(&self) -> &'static [Action] {
        const { &[Action::default("Copy to clipboard", "")] }
    }
}

fn roll(s: &str) -> Option<(Entry, usize)> {
    let (dice, sides) = s.split_once('d')?;
    let dice: usize = dice.trim().parse().ok()?;
    let sides: usize = sides.trim().parse().ok()?;
    if sides < 1 {
        return None;
    }

    let mut result = 0usize;
    let mut subtitle = String::from("Rolls:");
    let mut rng = rand::rng();

    for i in 0..dice {
        let res = rng.random_range(1..=sides);
        if i != 0 {
            subtitle.push(',');
        }
        subtitle.push(' ');
        result += res;
        _ = write!(subtitle, "{}", res);
    }

    Some((
        Entry {
            name: format!("Rolled {}d{} - Total: {}", dice, sides, result),
            subtitle: subtitle.into(),
            plugin: DicePlugin.prefix(),
            data: CustomData::new(result),
        },
        result,
    ))
}
