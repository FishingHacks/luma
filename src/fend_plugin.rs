use std::{
    collections::HashMap,
    sync::{
        Arc, LazyLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use fend_core::{Context, Interrupt};
use iced::{Task, clipboard};
use serde::Deserialize;
use tokio::sync::RwLock;

use crate::{
    Action, CustomData, Entry, Message, StructPlugin, cache::HTTPCache,
    filter_service::ResultBuilderRef, matcher::MatcherInput, utils,
};

#[derive(Default)]
pub struct FendPlugin(RwLock<Context>);

// TODO: currency handler

impl Interrupt for ResultBuilderRef<'_> {
    fn should_interrupt(&self) -> bool {
        self.should_stop()
    }
}

const REFRESH_TIMEOUT: Duration = /* 24 hours*/ Duration::from_secs(60 * 60 * 24);

static GETTING_CURRENCIES: AtomicBool = AtomicBool::new(false);
static CURRENCIES: LazyLock<RwLock<HashMap<String, f64>>> = LazyLock::new(<_>::default);

struct ExchangeRateHandler;

impl fend_core::ExchangeRateFnV2 for ExchangeRateHandler {
    fn relative_to_base_currency(
        &self,
        currency: &str,
        _: &fend_core::ExchangeRateFnV2Options,
    ) -> Result<f64, Box<dyn std::error::Error + Send + Sync + 'static>> {
        CURRENCIES
            .try_read()
            .ok()
            .and_then(|v| v.get(currency).copied())
            .ok_or_else(|| "".into())
    }
}

impl StructPlugin for FendPlugin {
    fn actions(&self) -> &[Action] {
        const {
            &[
                Action::default("Copy Value", "copy"),
                Action::suggest("Suggest Value", "suggest").keep_open(),
                Action::without_shortcut("About Fend", "fend").keep_open(),
                Action::without_shortcut("About Exchangerate API", "exchangerate").keep_open(),
            ]
        }
    }

    fn prefix() -> &'static str {
        "fend"
    }

    async fn get_for_values(
        &self,
        input: &MatcherInput,
        builder: ResultBuilderRef<'_>,
        _: crate::PluginContext<'_>,
    ) {
        // for some reason rust doesn't like this block not being here :< [it thinks the writer is
        // being dropped after the await, even tho it gets moved into the drop function?]
        let Ok(result) =
            fend_core::evaluate_with_interrupt(input.input(), &mut *self.0.write().await, &builder)
        else {
            return;
        };
        let result = result.get_main_result().trim();
        if result.is_empty() {
            return;
        }
        let result: Arc<str> = result.into();
        builder
            .add(Entry {
                name: result.clone().into(),
                subtitle: "exchange rates by exchangerate-api.com • powered by fend".into(),
                perfect_match: true,
                data: CustomData::new(result),
            })
            .await;
    }

    fn handle_pre(
        &self,
        thing: CustomData,
        action: &str,
        _: crate::PluginContext<'_>,
    ) -> Task<Message> {
        let v = thing.into::<Arc<str>>();
        match action {
            "copy" => clipboard::write(v.to_string()),
            "suggest" => Task::done(Message::SetSearch(format!("fend {v}"))),
            "fend" => {
                utils::open_link("https://github.com/printfn/fend/");
                Task::none()
            }
            "exchangerate" => {
                utils::open_link("https://www.exchangerate-api.com/");
                Task::none()
            }
            _ => unreachable!(),
        }
    }

    async fn init(&mut self, ctx: crate::PluginContext<'_>) {
        self.0
            .write()
            .await
            .set_exchange_rate_handler_v2(ExchangeRateHandler);
        if !GETTING_CURRENCIES.swap(true, Ordering::Relaxed) {
            tokio::spawn(async move {
                let res = HTTPCache::get(
                    ctx.http_cache,
                    &ctx.sqlite,
                    "https://open.er-api.com/v6/latest/USD",
                    None,
                    Some(REFRESH_TIMEOUT),
                )
                .await;
                GETTING_CURRENCIES.store(false, Ordering::Relaxed);
                if !res.err.is_empty() {
                    log::error!("Failed to get the currency exchange rates: {}", res.err);
                    return;
                }
                let Ok(body) = str::from_utf8(&res.body) else {
                    log::error!("exchange rate api did not return valid utf-8");
                    return;
                };
                let Ok(resp) = serde_json::from_str::<ExchRateResp>(body) else {
                    log::error!("exchange rate api did not return a valid response");
                    return;
                };
                *CURRENCIES.write().await = resp.rates;
            });
        }
    }
}

#[derive(Deserialize)]
struct ExchRateResp {
    rates: HashMap<String, f64>,
}
