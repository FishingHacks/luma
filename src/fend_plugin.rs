use std::sync::{Arc, RwLock};

use fend_core::{Context, Interrupt};
use iced::{Task, clipboard};

use crate::{
    Action, CustomData, Entry, Message, Plugin, filter_service::ResultBuilderRef,
    matcher::MatcherInput,
};

#[derive(Default)]
pub struct FendPlugin(RwLock<Context>);

// TODO: currency handler

impl Interrupt for ResultBuilderRef<'_> {
    fn should_interrupt(&self) -> bool {
        self.should_stop()
    }
}

impl Plugin for FendPlugin {
    fn actions(&self) -> &[Action] {
        const {
            &[
                Action::default("Copy Value", "copy"),
                Action::suggest("Suggest Value", "suggest"),
            ]
        }
    }

    fn prefix(&self) -> &'static str {
        "fend"
    }

    async fn get_for_values(&self, input: &MatcherInput, builder: ResultBuilderRef<'_>) {
        // for some reason rust doesn't like this block not being here :< [it thinks the writer is
        // being dropped after the await, even tho it gets moved into the drop function?]
        let result = {
            let Ok(mut writer) = self.0.write() else {
                return;
            };
            let Ok(result) =
                fend_core::evaluate_with_interrupt(input.input(), &mut writer, &builder)
            else {
                return;
            };
            drop(writer);
            result
        };
        let result = result.get_main_result().trim();
        if result.is_empty() {
            return;
        }
        let result: Arc<str> = result.into();
        builder
            .add(Entry {
                name: result.clone().into(),
                subtitle: "powered by fend".into(),
                perfect_match: true,
                data: CustomData::new(result),
            })
            .await;
    }

    fn handle_pre(&self, thing: CustomData, action: &str) -> Task<Message> {
        let v = thing.into::<Arc<str>>();
        if action == "copy" {
            clipboard::write(v.to_string())
        } else {
            Task::done(Message::SetSearch(format!("fend {v}")))
        }
    }

    fn init(&mut self) {}
}
