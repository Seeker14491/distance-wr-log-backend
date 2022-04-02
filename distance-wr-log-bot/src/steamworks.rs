use anyhow::Result;
use futures::{future, Stream, TryStreamExt};
use serde_derive::{Deserialize, Serialize};
use steamworks::ugc::MatchingUgcType;
use steamworks::{Client, InitError};
use tap::Pipe;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaderboardResponse {
    pub entries: Box<[LeaderboardEntry]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaderboardEntry {
    pub steam_id: u64,
    pub global_rank: i32,
    pub score: i32,
    pub player_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkshopResponse {
    pub published_file_id: u64,
    pub steam_id_owner: u64,
    pub file_name: String,
    pub title: String,
    pub score: f32,
    pub tags: Box<[String]>,
    pub author_name: String,
    pub preview_url: String,
}

#[derive(Debug, Clone)]
pub struct Steamworks(Client);

impl Steamworks {
    pub fn new() -> Result<Self, InitError> {
        Ok(Steamworks(Client::init(Some(233610))?))
    }

    pub async fn get_leaderboard_range(
        &self,
        leaderboard_name: String,
        start: u32,
        end: u32,
    ) -> Result<LeaderboardResponse> {
        let leaderboard = self.0.find_leaderboard(leaderboard_name.clone()).await?;
        let entries = leaderboard
            .download_global(start, end, 0)
            .await
            .into_iter()
            .map(|entry| async move {
                let player_name = entry.steam_id.persona_name(&self.0).await;

                LeaderboardEntry {
                    steam_id: entry.steam_id.into(),
                    global_rank: entry.global_rank,
                    score: entry.score,
                    player_name,
                }
            })
            .pipe(future::join_all)
            .await
            .into_boxed_slice();

        Ok(LeaderboardResponse { entries })
    }

    pub fn get_all_workshop_sprint_challenge_stunt_levels(
        &self,
    ) -> impl Stream<Item = Result<WorkshopResponse>> + '_ {
        self.0
            .query_all_ugc(MatchingUgcType::ItemsReadyToUse)
            .match_any_tags()
            .required_tags(["Sprint", "Challenge", "Stunt"].iter().copied())
            .run()
            .try_filter(|details| future::ready(!details.file_name.is_empty()))
            .map_ok(move |details| {
                let tags: Vec<_> = details.tags.iter().map(|s| s.to_owned()).collect();
                async move {
                    let author_name = details.steam_id_owner.persona_name(&self.0).await;
                    Ok(WorkshopResponse {
                        published_file_id: details.published_file_id.into(),
                        steam_id_owner: details.steam_id_owner.into(),
                        file_name: details.file_name,
                        title: details.title,
                        score: details.score,
                        tags: tags.into_boxed_slice(),
                        author_name,
                        preview_url: details.preview_url,
                    })
                }
            })
            .try_buffer_unordered(usize::MAX)
            .err_into()
    }
}
