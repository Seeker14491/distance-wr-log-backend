use anyhow::Result;
use futures::{pin_mut, Stream, TryStreamExt};
use itertools::Itertools;
use serde_derive::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaderboardResponse {
    pub entries: Box<[LeaderboardEntry]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaderboardEntry {
    pub steam_id: u64,
    pub global_rank: i32,
    pub score: i32,
    pub player_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkshopResponse {
    pub published_file_id: u64,
    pub steam_id_owner: u64,
    pub file_name: String,
    pub title: String,
    pub score: f32,
    pub tags: Box<[String]>,
    pub author_name: Option<String>,
    pub preview_url: String,
}

#[derive(Debug, Clone)]
pub struct Steamworks {
    grpc_client: distance_steam_data_client::Client,
    web_client: reqwest::Client,
    web_api_key: String,
}

impl Steamworks {
    pub async fn new(
        grpc_address: impl Into<String>,
        web_api_key: impl Into<String>,
    ) -> Result<Self> {
        Ok(Steamworks {
            grpc_client: distance_steam_data_client::Client::connect(grpc_address.into()).await?,
            web_client: reqwest::Client::new(),
            web_api_key: web_api_key.into(),
        })
    }

    pub async fn get_leaderboard_range(
        &self,
        leaderboard_name: &str,
        start: i32,
        end: i32,
    ) -> Result<LeaderboardResponse> {
        let entries = self
            .grpc_client
            .leaderboard_entries_range(leaderboard_name, start, end)
            .await?
            .entries
            .into_iter()
            .map(|entry| LeaderboardEntry {
                steam_id: entry.steam_id,
                global_rank: entry.global_rank,
                score: entry.score,
                player_name: None,
            })
            .collect_vec()
            .into_boxed_slice();

        Ok(LeaderboardResponse { entries })
    }

    pub fn get_all_workshop_sprint_challenge_stunt_levels(
        &self,
    ) -> impl Stream<Item = Result<WorkshopResponse>> + '_ {
        ez_stream::try_unbounded(move |tx| async move {
            let stream = steam_workshop::query_all_files(
                self.web_client.clone(),
                self.web_api_key.clone(),
                233610,
            );
            pin_mut!(stream);
            while let Some(chunk) = stream.try_next().await? {
                for details in chunk {
                    let is_relevant_level = details
                        .tags
                        .iter()
                        .any(|tag| ["Sprint", "Challenge", "Stunt"].contains(&tag.tag.as_str()));
                    if !is_relevant_level || details.filename.is_empty() {
                        continue;
                    }

                    tx.send(WorkshopResponse {
                        published_file_id: details.published_file_id,
                        steam_id_owner: details.creator,
                        file_name: details.filename,
                        title: details.title,
                        score: details.vote_data.score,
                        tags: details.tags.into_iter().map(|tag| tag.tag).collect(),
                        author_name: None,
                        preview_url: details.preview_url,
                    })?;
                }
            }

            Ok(())
        })
    }

    pub async fn resolve_steam_names(
        &self,
        steam_ids: Vec<u64>,
    ) -> Result<impl Iterator<Item = Option<String>>> {
        self.grpc_client.persona_names(steam_ids).await
    }
}
