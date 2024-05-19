use std::sync::Arc;
use axum::{body::Body, http::Response, routing::get, Extension, Router};
use chrono::TimeDelta;
use rspotify::{
    clients::{BaseClient, OAuthClient},
    model::{AdditionalType, Device, FullTrack, RepeatState},
    scopes, AuthCodeSpotify, Credentials, Token,
};
use serde::Serialize;
use tokio::sync::Mutex; 

#[derive(Debug, Clone)]
struct PlaybackState {
    is_playing: bool,
    position: u64,
    device_id: String,
}

#[derive(Debug, Clone)]
struct SpotifyState {
    spotify: AuthCodeSpotify,
    playback_status: Option<PlaybackState>,
}

#[derive(Debug, Clone, Serialize)]
struct Artist {
    name: String,
    url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct Track {
    name: String,
    artists: Vec<Artist>,
    image_url: Option<String>,
    url: Option<String>,
    duration: u32,
}

#[derive(Debug, Clone, Serialize)]
struct CurrentlyPlaying {
    device: Device,
    track: Track,
    progress_secs: u32,
    shuffled: bool,
    playing: bool,
    repeat_status: RepeatState,
}

impl Track {
    async fn simplify_track(full_track: FullTrack) -> Self {
        Self {
            name: full_track.name,
            artists: full_track
                .artists
                .into_iter()
                .map(|artist| Artist {
                    name: artist.name,
                    url: artist.external_urls.get("spotify").cloned(),
                })
                .collect(),
            image_url: Some(full_track.album.images[0].url.clone()),
            url: full_track.external_urls.get("spotify").cloned(),
            duration: full_track.duration.num_seconds() as u32,
        }
    }
}

async fn update_state(state: Extension<Arc<Mutex<SpotifyState>>>) {
    let mut locked_state = state.lock().await;
    
    let currently_playing_res = locked_state
        .spotify
        .current_playback(
            None,
            Some(&[AdditionalType::Track, AdditionalType::Episode]),
        )
        .await;

        match currently_playing_res {
            Ok(Some(playing)) => {
                locked_state.playback_status = Some(PlaybackState {
                   is_playing: playing.is_playing,
                   position: playing.progress.unwrap().num_seconds() as u64,
                   device_id: playing.device.clone().id.unwrap(),
                   
                });
            }
            Ok(None) => {
                locked_state.playback_status = None;
            }
            Err(_) => {
                locked_state.playback_status = None;
            }
        }


}

async fn get_current_playback(state: Extension<Arc<Mutex<SpotifyState>>>) -> Result<Response<Body>, String> {
    let mut locked_state = state.lock().await;

    // TODO: In attempts to not call spotify api as often, make it so it only updates every 5 seconds

    let currently_playing_res = locked_state
        .spotify
        .current_playback(
            None,
            Some(&[AdditionalType::Track, AdditionalType::Episode]),
        )
        .await;

    match currently_playing_res {
        Ok(Some(playing)) => {
            let track_info = match playing.item.unwrap().id().unwrap() {
                rspotify::model::PlayableId::Track(track_id) => locked_state.spotify
                    .track(track_id, None)
                    .await
                    .expect("Could not get information for track"),

                rspotify::model::PlayableId::Episode(_) => {
                    unreachable!("Does not parse episodes");
                }
            };

            locked_state.playback_status = Some(PlaybackState {
               is_playing: playing.is_playing,
               position: playing.progress.unwrap().num_seconds() as u64,
               device_id: playing.device.clone().id.unwrap(),
            });

            let res_playing = CurrentlyPlaying {
                device: playing.device,
                track: Track::simplify_track(track_info).await,
                progress_secs: playing.progress.unwrap().num_seconds() as u32,
                shuffled: playing.shuffle_state,
                playing: playing.is_playing,
                repeat_status: playing.repeat_state,
            };

            let body = serde_json::to_string(&res_playing).unwrap();

            Ok(Response::builder()
                .header("Content-Type", "application/json")
                .body(Body::from(body))
                .unwrap())
        }
        Ok(None) => {
            locked_state.playback_status = None;

            Ok(Response::builder()
                .header("Content-Type", "application/json")
                .body(Body::new(
                    "{\"message\": \"Could not get playback\"}".to_string(),
                ))
                .unwrap())
        }
        Err(err) => {
            locked_state.playback_status = None;
            
            Err(format!("Error with getting playback, {}", err))
        }
    }
}

async fn toggle_playback(state: Extension<Arc<Mutex<SpotifyState>>>) {
    update_state(state.clone()).await;

    let locked_state = state.lock().await;

    if let Some(mut playback) = locked_state.playback_status.clone() {
        if playback.is_playing {
            let _ = locked_state
                .spotify
                .pause_playback(Some(playback.device_id.as_str())).await;
            playback.is_playing = false;
        } else {
            let _ = locked_state
                .spotify
                .resume_playback(
                    Some(playback.device_id.as_str()), 
                    Some(TimeDelta::new(playback.position as i64, 0).unwrap())
                ).await;
            
            playback.is_playing = false;
        }
    }
}

#[tokio::main]
async fn main() {
    let creds = Credentials::from_env().unwrap();
    let mut spotify = AuthCodeSpotify::from_token(Token {
        refresh_token: Some(std::env::var("REFRESH_TOKEN").unwrap()),
        scopes: scopes!(
            "user-read-currently-playing",
            "user-read-playback-position",
            "user-read-playback-state"
        ),
        ..Default::default()
    });

    spotify.creds = creds;
    spotify.refresh_token().await.unwrap();

    let shared_state = Arc::new(Mutex::new(SpotifyState {
        spotify,
        playback_status: None,
    }));

    update_state(Extension(shared_state.clone())).await;

    let app = Router::new()
        .route("/current_playback", get(get_current_playback))
        .route("/toggle_playback", get(toggle_playback))
        .layer(Extension(shared_state));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();

    axum::serve(listener, app).await.unwrap();
}
