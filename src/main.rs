#![allow(warnings)]

use axum::{
    body::{Body, HttpBody},
    http::{Request, Response, StatusCode},
    routing::{get, post},
    Json, Router,
};
use rspotify::{
    clients::{BaseClient, OAuthClient},
    scopes, AuthCodeSpotify, Config, Credentials, OAuth, Token,
};
use std::{env, thread::scope};

async fn root() {}

#[tokio::main]
async fn main() {
    let creds = Credentials::from_env().unwrap();
    let mut spotify = AuthCodeSpotify::from_token(Token {
        refresh_token: Some(std::env::var("REFRESH_TOKEN").unwrap()),
        scopes: scopes!("user-read-currently-playing"),
        ..Default::default()
    });

    spotify.creds = creds;
    spotify.refresh_token().await.unwrap();

    let app = Router::new().route("/", get(root));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();

    axum::serve(listener, app).await.unwrap();
}
