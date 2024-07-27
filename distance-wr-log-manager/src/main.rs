#![warn(
    deprecated_in_future,
    macro_use_extern_crate,
    missing_debug_implementations,
    unused_qualifications
)]

use anyhow::{format_err, Context, Error, Result};
use backoff::backoff::Backoff;
use backoff::ExponentialBackoff;
use futures::pin_mut;
use log::{error, info, warn};
use std::fmt::Display;
use std::process::ExitStatus;
use std::time::{Duration, Instant};
use std::{env, process};
use tokio::process::Command;
use tokio::time;

const UPDATE_PERIOD: Duration = Duration::from_secs(5 * 60);
const MAX_UPDATE_DURATION: Duration = Duration::from_secs(60 * 60);

#[tokio::main(flavor = "current_thread")]
async fn main() {
    color_backtrace::install();
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let healthchecks_url = match env::var("HEALTHCHECKS_URL") {
        Ok(x) => Some(x),
        Err(e) => {
            match e {
                env::VarError::NotPresent => {
                    warn!("Environment variable HEALTHCHECKS_URL is not set")
                }
                env::VarError::NotUnicode(_) => {
                    warn!("Invalid HEALTHCHECKS_URL environment variable");
                }
            }

            None
        }
    };

    let result = run(healthchecks_url.as_deref()).await;

    if let Err(e) = result {
        if let Some(url) = healthchecks_url {
            healthchecks_send_fail_signal(&url, &format!("error: {e}"))
                .await
                .expect("Couldn't send healthchecks fail signal");
        }

        print_error(e);

        process::exit(-1);
    }
}

fn print_error<E: Into<Error>>(e: E) {
    let e = e.into();
    error!("error: {}", e);
    while let Some(e) = e.source() {
        error!(" caused by: {}", e);
    }
}

async fn run(healthchecks_url: Option<&str>) -> Result<()> {
    let mut backoff = ExponentialBackoff {
        max_elapsed_time: None,
        ..Default::default()
    };
    loop {
        let update_start_time = Instant::now();
        let f = run_distance_log();
        pin_mut!(f);
        match time::timeout(MAX_UPDATE_DURATION, f).await {
            Ok(Ok(exit_status)) if exit_status.success() => {
                if let Some(url) = healthchecks_url {
                    healthchecks_send_ping(url).await.ok();
                }

                time::sleep(
                    UPDATE_PERIOD
                        .checked_sub(update_start_time.elapsed())
                        .unwrap_or_default(),
                )
                .await;
                backoff.reset();
            }
            Ok(_) => {
                print_error(format_err!("distance-wr-log-bot did not run successfully"));
                time::sleep(backoff.next_backoff().unwrap()).await;
            }
            Err(_) => {
                print_error(format_err!("distance-wr-log-bot ran for too long"));
                backoff.reset();
            }
        }
    }
}

async fn run_distance_log() -> Result<ExitStatus> {
    info!("Starting distance-wr-log-bot");
    let mut child = Command::new("./distance-wr-log-bot")
        .spawn()
        .context("Couldn't spawn the distance-wr-log-bot process")?;

    Ok(child.wait().await?)
}

async fn healthchecks_send_ping(healthchecks_url: &str) -> Result<()> {
    let err_msg = "error sending fail signal";

    reqwest::get(healthchecks_url)
        .await
        .context(err_msg)?
        .error_for_status()
        .context(err_msg)?;

    Ok(())
}

async fn healthchecks_send_fail_signal(
    healthchecks_url: &str,
    error: impl Display,
) -> Result<(), Error> {
    let client = reqwest::Client::new();
    client
        .post(format!("{healthchecks_url}/fail"))
        .body(format!("[manager] error: {error}"))
        .send()
        .await
        .map_err(|e| format_err!("Error sending fail signal: {}", e))?;

    Ok(())
}
