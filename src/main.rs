use axum::{
    body::Body, extract::State, http::Response, routing::get, Router
};
use rspotify::{
    clients::{BaseClient, OAuthClient},
    model::{AdditionalType, Device, FullTrack},
    scopes, AuthCodeSpotify, Credentials, Token,
};
use serde::Serialize;

#[derive(Debug, Clone)]
struct SpotifyState {
    spotify: AuthCodeSpotify,
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
    playing: bool
}

async fn simplify_track(full_track: FullTrack) -> Track {
    Track {
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

async fn get_current_playback(State(state): State<SpotifyState>) -> Result<Response<Body>, String> {
    let spotify = state.spotify;

    // TODO: In attempts to not call spotify api as often, make it so it only updates every 5 seconds

    let currently_playing_res = spotify
        .current_playback(
            None,
            Some(&[AdditionalType::Track, AdditionalType::Episode]),
        )
        .await;

    match currently_playing_res {
        Ok(Some(playing)) => {
            let track_info = match playing.item.unwrap().id().unwrap() {
                rspotify::model::PlayableId::Track(track_id) => spotify
                    .track(track_id, None)
                    .await
                    .expect("Could not get information for track"),

                rspotify::model::PlayableId::Episode(_) => {
                    unreachable!("Does not parse episodes");
                }
            };

            let res_playing = CurrentlyPlaying {
                device: playing.device,
                track: simplify_track(track_info).await,
                progress_secs: playing.progress.unwrap().num_seconds() as u32,
                shuffled: playing.shuffle_state,
                playing: playing.is_playing
            };

            let body = serde_json::to_string(&res_playing).unwrap();

            return Ok(Response::builder()
                .header("Content-Type", "application/json")
                .body(Body::from(body))
                .unwrap());
            
        }
        Ok(None) => {
            return Ok(Response::builder()
                .header("Content-Type", "application/json")
                .body(Body::new("{\"message\": \"Could not get playback\"}".to_string()))
                .unwrap());
        }
        Err(err) => {
            return Err(format!("Error with getting playback, {}", err));
        }
    }
}

async fn root() {}

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

    let shared_state = SpotifyState { spotify };

    let app = Router::new()
        .route("/", get(root))
        .route("/current_playback", get(get_current_playback))
        .with_state(shared_state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();

    axum::serve(listener, app).await.unwrap();
}
