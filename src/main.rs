use std::{default, future::IntoFuture};

use axum::{extract::State, routing::get, Router};
use rspotify::{
    clients::{BaseClient, OAuthClient},
    model::{track, AdditionalType, Country, Device, FullEpisode, FullTrack, Market, SimplifiedArtist, SimplifiedTrack},
    scopes, AuthCodeSpotify, Credentials, Token,
};

#[derive(Debug, Clone)]
struct SpotifyState {
    spotify: AuthCodeSpotify,
}

#[derive(Debug, Clone)]
struct Artist {
    name: String,
    url: Option<String>
}

#[derive(Debug, Clone)]
struct Track {
    name: String,
    artists: Vec<Artist>,
    image_url: Option<String>,
    url: Option<String>,
    duration: u32
}

#[derive(Debug, Clone)]
struct CurrentlyPlaying {
    device: Device,
    track: Track,
    progress_secs: u32,
    shuffled: bool
}

async fn simplify_track(full_track: FullTrack) -> Track {
    Track {
        name: full_track.name,
        artists: full_track.artists.into_iter().map(|artist| Artist {
            name: artist.name,
            url: artist.external_urls.get("spotify").cloned()
        }).collect(),
        image_url: Some(full_track.album.images[0].url.clone()),
        url: full_track.external_urls.get("spotify").cloned(),
        duration: full_track.duration.num_seconds() as u32
    }
}


async fn get_current_playback(State(state): State<SpotifyState>) {
    let spotify = state.spotify;

    // TODO: In attempts to not call spotify api as often, make it so it only updates every 5 seconds

    let currently_playing_res = spotify
        .current_playback(
            None,
            Some(&[AdditionalType::Track, AdditionalType::Episode]),
        )
        .await;

    println!("Response: {currently_playing_res:#?}");
    match currently_playing_res {
        Ok(Some(playing)) => {

            let track_info = match playing.item.unwrap().id().unwrap() {
                rspotify::model::PlayableId::Track(track_id) => {
                    spotify.track(track_id, None).await.expect(format!("Could not get information for track").as_str())
                }

                rspotify::model::PlayableId::Episode(_) => {
                    unreachable!("Does not parse episodes");
                }
            };

            let res_playing = CurrentlyPlaying {
                device: playing.device,
                track: simplify_track(track_info).await,
                progress_secs: playing.progress.unwrap().num_seconds() as u32,
                shuffled: playing.shuffle_state,
            };

            println!("res: {:?}", res_playing);
        }
        Ok(None) => {
            println!("Not playing anything");
        }
        Err(err) => {
            panic!("Error with getting playback, {}", err);
        }
    }
    // if let Some(currently_playing) = currently_playing {
    //     println!("Playing something!");
    // } else {
    //     println!("Not playing anything!")
    // }
}

async fn root() {}

#[tokio::main]
async fn main() {
    let creds = Credentials::from_env().unwrap();
    println!("{}", std::env::var("REFRESH_TOKEN").unwrap());
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

    let currently_playing_res = spotify
    .current_playback(
        None,
        Some(&[AdditionalType::Track, AdditionalType::Episode]),
    )
    .await;

    let shared_state = SpotifyState { spotify };

    let app = Router::new()
        .route("/", get(root))
        .route("/current_playback", get(get_current_playback))
        .with_state(shared_state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();

    axum::serve(listener, app).await.unwrap();
}
