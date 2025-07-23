use iced::clipboard;
use rand::Rng;
use std::fmt::Write;

use crate::{
    Action, CustomData, Entry, ResultBuilderRef, StructPlugin, matcher::MatcherInput,
    plugin::StringLike,
};

#[derive(Default)]
pub struct DicePlugin;

impl StructPlugin for DicePlugin {
    fn prefix() -> &'static str {
        "roll"
    }

    async fn get_for_values(
        &self,
        input: &MatcherInput,
        builder: ResultBuilderRef<'_>,
        _: crate::PluginContext<'_>,
    ) {
        let words = input.words();
        if words.is_empty() {
            return;
        }
        let mut entries = Vec::with_capacity(words.len());
        let mut total = 0;
        for entry in words.iter().map(|v| v as &str).filter_map(roll) {
            entries.push(entry.0);
            total += entry.1;
        }
        if entries.len() > 1 {
            entries.insert(
                0,
                Entry::new(
                    format!("Overall Total:  {total}"),
                    StringLike::Empty,
                    CustomData::new(total),
                ),
            );
        }
        builder.commit(entries.into_iter()).await;
    }

    async fn init(&mut self, _: crate::PluginContext<'_>) {}

    fn handle_pre(
        &self,
        thing: crate::CustomData,
        _: &str,
        _: crate::PluginContext<'_>,
    ) -> iced::Task<crate::Message> {
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
        _ = write!(subtitle, "{res}");
    }

    let name = format!("Rolled {dice}d{sides} - Total: {result}");
    Some((Entry::new(name, subtitle, CustomData::new(result)), result))
}
