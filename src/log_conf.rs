use std::str::FromStr;

use tracing::Level;
use tracing_subscriber::{
    fmt::time::FormatTime, layer::SubscriberExt as _, util::SubscriberInitExt as _, Layer as _,
};

struct LocalTimer;

impl FormatTime for LocalTimer {
    fn format_time(&self, w: &mut tracing_subscriber::fmt::format::Writer<'_>) -> std::fmt::Result {
        write!(w, "[{}]", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"))
    }
}

pub fn init_tracing_subscriber(targets: &[&'static str]) {
    // let (non_blocking_out, guard_out) = tracing_appender::non_blocking(stdout());
    // let (non_blocking_err, guard_err) = tracing_appender::non_blocking(stderr());
    let log_level = std::env::var("RUST_LOG");
    let log_level = match log_level {
        Ok(level) => Level::from_str(level.as_str()).unwrap_or(Level::DEBUG),
        Err(_) => Level::DEBUG,
    };
    let out_filter = {
        let filter = tracing_subscriber::filter::Targets::new()
            .with_target("tracing_log", log_level)
            .with_target("panic", log_level);
        targets.iter().fold(filter, |filter, target| {
            filter.with_target(*target, log_level)
        })
    };
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_timer(LocalTimer)
                .with_line_number(true)
                .with_target(true)
                // .with_writer(non_blocking_out)
                .compact()
                .with_filter(out_filter),
        )
        .init();
}
