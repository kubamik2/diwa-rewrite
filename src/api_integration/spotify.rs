use rspotify::{
    model::{ PlaylistId, TrackId, PlayableItem, AlbumId, FullTrack, SimplifiedTrack },
    prelude::*,
    scopes, Credentials, OAuth, ClientCredsSpotify
};
use crate::error::{ Error, AppError };
use thiserror::Error as ThisError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpotifyTrackData {
    pub title: String,
    pub artists: Vec<String>
}

#[derive(Debug, ThisError)]
pub enum SpotifyApiError {
    #[error("playlist is empty")]
    EmptyPlaylist
}

impl From<FullTrack> for SpotifyTrackData {
    fn from(value: FullTrack) -> Self {
        let title = value.name.to_owned();
        let artists = value.artists.iter().map(|artist| artist.name.clone()).collect::<Vec<String>>();
        Self { title, artists }
    }
}

impl From<&FullTrack> for SpotifyTrackData {
    fn from(value: &FullTrack) -> Self {
        let title = value.name.to_owned();
        let artists = value.artists.iter().map(|artist| artist.name.clone()).collect::<Vec<String>>();
        Self { title, artists }
    }
}

impl From<&SimplifiedTrack> for SpotifyTrackData {
    fn from(value: &SimplifiedTrack) -> Self {
        let title = value.name.to_owned();
        let artists = value.artists.iter().map(|artist| artist.name.clone()).collect::<Vec<String>>();
        Self { title, artists }
    }
}

pub struct SpotifyClient {
    client: ClientCredsSpotify
}

impl SpotifyClient {
    pub fn new() -> Result<Self, Error> {
        let creds = Credentials::from_env().ok_or(AppError::MissingEnvEntries { entries: vec!["RSPOTIFY_CLIENT_ID".to_owned(), "RSPOTIFY_CLIENT_SECRET".to_owned()] })?;
        OAuth::from_env(scopes!("playlist-read-private","playlist-read-collaborative","user-read-private","user-library-read")).ok_or(AppError::MissingEnvEntry { entry: "RSPOTIFY_REDIRECT_URI".to_owned() })?;
        let mut client = ClientCredsSpotify::new(creds);
        client.request_token()?;
        client.config.token_refreshing = true;
        Ok(Self { client })
    }

    pub fn track(&self, id: &str) -> Result<SpotifyTrackData, Error> {
        let track_id = TrackId::from_id(id)?;
        let track = self.client.track(track_id)?;

        Ok(SpotifyTrackData::from(track))
    }

    pub fn playlist(&self, id: &str) -> Result<Vec<SpotifyTrackData>, Error> {
        let playlist_id = PlaylistId::from_id(id)?;
        let playlist = self.client.playlist(playlist_id, None, None)?;
        let mut tracks = vec![];

        for item in playlist.tracks.items.iter() {
            if let Some(playable_item) = &item.track {
                match playable_item {
                    PlayableItem::Track(track) => {
                        tracks.push(SpotifyTrackData::from(track))
                    },
                    PlayableItem::Episode(_) => {
                        return Err(AppError::UnsupportedContent { content: "Spotify Episode".to_owned() }.into());
                    }
                }
            }
        }
        
        if tracks.is_empty() { return Err(SpotifyApiError::EmptyPlaylist.into()); }
        Ok(tracks)
    }

    pub fn album(&self, id: &str) -> Result<Vec<SpotifyTrackData>, Error> {
        let album_id = AlbumId::from_id(id)?;
        let album = self.client.album(album_id)?;
        let mut tracks = vec![];

        for item in album.tracks.items.iter() {
            tracks.push(SpotifyTrackData::from(item));
        }

        if tracks.is_empty() { return Err(SpotifyApiError::EmptyPlaylist.into()); }
        Ok(tracks)
    }
}