#![warn(
    deprecated_in_future,
    macro_use_extern_crate,
    missing_debug_implementations,
    unused_qualifications
)]

use crate::domain::{ChangelistEntry, LevelInfo};
use crate::file_json_persistence::{FileJsonPersistence, LoadError};
use crate::steamworks::Steamworks;
use anyhow::{Context, Result};
use chrono::Utc;
use distance_util::LeaderboardGameMode;
use futures::{future, stream, Stream, StreamExt, TryStreamExt};
use if_chain::if_chain;
use indicatif::ProgressBar;
use itertools::{EitherOrBoth, Itertools};
use log::{info, warn};
use std::collections::BTreeMap;
use std::future::Future;
use std::path::Path;
use std::time::Duration;
use tap::Pipe;

mod domain;
mod file_json_persistence;
mod official_levels;
mod steamworks;

const QUERY_RESULTS_PATH: &str = "/data/query_results.json";
const CHANGELIST_PATH: &str = "/data/changelist.json";

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let steamworks = Steamworks::new()?;
    let persistence = FileJsonPersistence::new(QUERY_RESULTS_PATH, CHANGELIST_PATH);

    info!("Starting update procedure");
    update(&steamworks, &persistence).await?;
    info!("Finished update procedure");

    Ok(())
}

async fn update(steamworks: &Steamworks, persistence: &FileJsonPersistence) -> Result<()> {
    let old_level_infos = match persistence.load_query_results() {
        Ok(x) => {
            info!("Loaded previous query results");
            Some(x)
        }
        Err(e) => {
            if let LoadError::DoesNotExist = e {
                warn!("No previous query results found");
                None
            } else {
                return Err(e).context("Error loading query results");
            }
        }
    };

    let mut changelist = match persistence.load_changelist() {
        Ok(x) => {
            info!("Loaded changelist");
            x
        }
        Err(e) => {
            if let LoadError::DoesNotExist = e {
                warn!("No existing changelist found");
                Vec::new()
            } else {
                return Err(e).context("Error loading changelist");
            }
        }
    };

    let spinner = ProgressBar::new_spinner();
    let mut new_level_infos = get_level_infos(steamworks)
        .inspect(|res| {
            if let Ok(level_info) = res {
                spinner.set_message(format!("Fetched level {}", &level_info.name));
            }
        })
        .try_collect::<Vec<_>>()
        .await?;
    spinner.finish_with_message("Finished fetching level information.");

    // Deal with Steam sometimes failing to return data by supplementing it with the previously stored
    // data.
    if let Some(ref old) = old_level_infos {
        new_level_infos = add_missing_entries_from(new_level_infos, old.clone());
    }

    if let Some(old_level_infos) = old_level_infos {
        info!("Computing changelist");
        update_changelist(&mut changelist, &mut new_level_infos, old_level_infos);
    }

    info!("Saving changelist");
    persistence.save_changelist(&changelist)?;

    info!("Saving level info");
    persistence.save_query_results(&new_level_infos)?;

    Ok(())
}

fn get_level_infos(steamworks: &Steamworks) -> impl Stream<Item = Result<LevelInfo>> + '_ {
    const MAX_BUFFER: usize = 512;
    const TIMEOUT: Duration = Duration::from_secs(60);

    let official_levels = get_official_levels(steamworks)
        .pipe(stream::iter)
        .buffer_unordered(MAX_BUFFER);
    let workshop_levels = get_workshop_levels(steamworks)
        .buffer_unordered(MAX_BUFFER)
        .filter_map(|x| future::ready(x.transpose()));

    official_levels.chain(workshop_levels).pipe(|stream| {
        tokio_stream::StreamExt::timeout(stream, TIMEOUT)
            .take_while(|timeout_result| {
                let timed_out = timeout_result.is_err();
                if timed_out {
                    warn!("Skipping some levels that took too long to fetch");
                }

                future::ready(!timed_out)
            })
            .map(|timeout_result| timeout_result.unwrap())
    })
}

fn add_missing_entries_from(mut new: Vec<LevelInfo>, mut old: Vec<LevelInfo>) -> Vec<LevelInfo> {
    let sort = |x: &mut [LevelInfo]| {
        x.sort_unstable_by(|a, b| a.leaderboard_name.cmp(&b.leaderboard_name))
    };

    sort(&mut new);
    sort(&mut old);

    new.into_iter()
        .merge_join_by(old, |a, b| a.leaderboard_name.cmp(&b.leaderboard_name))
        .map(|x| match x {
            EitherOrBoth::Both(new, old) => {
                if new.leaderboard_response.entries.len() == 0
                    && old.leaderboard_response.entries.len() > 0
                {
                    old
                } else {
                    new
                }
            }
            EitherOrBoth::Left(x) | EitherOrBoth::Right(x) => x,
        })
        .collect()
}

fn get_official_levels(
    steamworks: &Steamworks,
) -> impl Iterator<Item = impl Future<Output = Result<LevelInfo>> + '_> + '_ {
    official_levels::iter().map(move |(level_name, mode)| {
        let leaderboard_name = distance_util::create_leaderboard_name_string(
            level_name, mode, None,
        )
        .unwrap_or_else(|| {
            panic!(
                "Couldn't create a leaderboard name string for the official level '{}'",
                level_name
            )
        });

        async move {
            let leaderboard_response = steamworks
                .get_leaderboard_range(leaderboard_name.clone(), 1, 2)
                .await?;

            Ok(LevelInfo {
                name: level_name.to_owned(),
                mode,
                leaderboard_name,
                workshop_response: None,
                leaderboard_response,
                timestamp: Utc::now(),
            })
        }
    })
}

fn get_workshop_levels(
    steamworks: &Steamworks,
) -> impl Stream<Item = impl Future<Output = Result<Option<LevelInfo>>> + '_> + '_ {
    let level_infos = steamworks
        .get_all_workshop_sprint_challenge_stunt_levels()
        .map_ok(|workshop_response| {
            [
                LeaderboardGameMode::Sprint,
                LeaderboardGameMode::Challenge,
                LeaderboardGameMode::Stunt,
            ]
            .iter()
            .filter_map(move |mode| {
                let level_supports_mode =
                    workshop_response.tags.iter().any(|tag| tag == mode.name());
                if !(level_supports_mode) {
                    return None;
                }

                distance_util::create_leaderboard_name_string(
                    remove_bytes_extension(&workshop_response.file_name),
                    *mode,
                    Some(workshop_response.steam_id_owner),
                )
                .map(|leaderboard_name| (workshop_response.clone(), *mode, leaderboard_name))
            })
            .map(Ok)
            .pipe(stream::iter)
        })
        .try_flatten();

    level_infos.map(|x: Result<_>| async {
        let (workshop_response, mode, leaderboard_name) = x?;
        steamworks
            .get_leaderboard_range(leaderboard_name.clone(), 1, 2)
            .await
            .ok()
            .map(|leaderboard_response| LevelInfo {
                name: workshop_response.title.clone(),
                mode,
                leaderboard_name,
                workshop_response: Some(workshop_response),
                leaderboard_response,
                timestamp: Utc::now(),
            })
            .pipe(Ok)
    })
}

fn update_changelist(
    changelist: &mut Vec<ChangelistEntry>,
    new: &mut [LevelInfo],
    old: Vec<LevelInfo>,
) {
    new.sort_by_key(|level_info| {
        level_info
            .workshop_response
            .as_ref()
            .map(|x| x.published_file_id)
            .unwrap_or(0)
    });
    let old: BTreeMap<_, _> = old
        .into_iter()
        .map(|level_info| (level_info.leaderboard_name.clone(), level_info))
        .collect();

    let entries = new.iter().filter_map(|level_info| {
        let LevelInfo {
            name,
            mode,
            leaderboard_name,
            workshop_response,
            leaderboard_response,
            timestamp,
        } = level_info;
        let first_entry = if let Some(x) = leaderboard_response.entries.get(0) {
            x.clone()
        } else {
            return None;
        };

        let (old_recordholder, record_old, steam_id_old_recordholder) = if_chain! {
            if let Some(level_info_old) = old.get(leaderboard_name);
            if let Some(previous_first_entry) = level_info_old.leaderboard_response.entries.get(0);
            then {
                if is_score_better(first_entry.score, previous_first_entry.score, *mode) {
                    (Some(previous_first_entry.player_name.clone()),
                        Some(distance_util::format_score(previous_first_entry.score, *mode).unwrap()),
                        Some(format!("{}", previous_first_entry.steam_id)))
                } else {
                    return None;
                }
            } else {
                (None, None, None)
            }
        };

        Some(ChangelistEntry {
            map_name: name.clone(),
            map_author: workshop_response.as_ref().map(|x| x.author_name.clone()),
            map_preview: workshop_response.as_ref().map(|x| x.preview_url.clone()),
            mode: format!("{}", mode),
            new_recordholder: first_entry.player_name,
            old_recordholder,
            record_new: distance_util::format_score(first_entry.score, *mode).unwrap(),
            record_old,
            workshop_item_id: workshop_response
                .as_ref()
                .map(|x| format!("{}", x.published_file_id)),
            steam_id_author: workshop_response
                .as_ref()
                .map(|x| format!("{}", x.steam_id_owner)),
            steam_id_new_recordholder: format!("{}", first_entry.steam_id),
            steam_id_old_recordholder,
            fetch_time: timestamp.to_rfc2822(),
        })
    });

    let entries: Vec<_> = entries
        .filter(|new_entry| {
            changelist
                .iter()
                .all(|existing_entry| !new_entry.is_likely_a_duplicate_of(existing_entry))
        })
        .rev()
        .collect();

    changelist.extend(entries);
}

fn is_score_better(this_score: i32, other_score: i32, game_mode: LeaderboardGameMode) -> bool {
    match game_mode {
        LeaderboardGameMode::Sprint | LeaderboardGameMode::Challenge => this_score < other_score,
        LeaderboardGameMode::Stunt => this_score > other_score,
    }
}

fn remove_bytes_extension(level: &str) -> &str {
    match Path::new(level).file_stem() {
        None => "",
        Some(s) => s.to_str().unwrap(),
    }
}

#[test]
fn test_remove_bytes_extension() {
    assert_eq!(remove_bytes_extension("some_level.bytes"), "some_level");
}
