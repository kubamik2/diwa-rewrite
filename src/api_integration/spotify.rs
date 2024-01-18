use rspotify::{
    model::{ PlaylistId, TrackId, PlayableItem, AlbumId, FullTrack, SimplifiedTrack },
    prelude::*,
    scopes, Credentials, OAuth, ClientCredsSpotify, ClientError
};
use thiserror::Error as ThisError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpotifyTrackData {
    pub title: String,
    pub artists: Vec<String>
}

#[derive(Debug, ThisError)]
pub enum SpotifyError {
    #[error("")]
    EmptyPlaylist,
    #[error("")]
    Api(rspotify::ClientError),
    #[error("")]
    Id(rspotify::model::IdError),
    #[error("")]
    EnvVarsMissing {vars: Vec<String>},
    #[error("Episodes are not supported")]
    EpisodesUnsupported,
    #[error("Provided playlist is private")]
    PlaylistPrivate,
}

impl From<rspotify::ClientError> for SpotifyError {
    fn from(value: rspotify::ClientError) -> Self {
        Self::Api(value)
    }
}

impl From<rspotify::model::IdError> for SpotifyError {
    fn from(value: rspotify::model::IdError) -> Self {
        Self::Id(value)
    }
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
    pub fn new() -> Result<Self, SpotifyError> {
        let creds = Credentials::from_env().ok_or(SpotifyError::EnvVarsMissing { vars: vec!["RSPOTIFY_CLIENT_ID".to_string(), "RSPOTIFY_CLIENT_SECRET".to_string()] })?;
        OAuth::from_env(scopes!("playlist-read-private","playlist-read-collaborative","user-read-private","user-library-read")).ok_or(SpotifyError::EnvVarsMissing { vars: vec!["RSPOTIFY_REDIRECT_URI".to_string()] })?;
        let mut client = ClientCredsSpotify::new(creds);
        client.request_token()?;
        client.config.token_refreshing = true;
        Ok(Self { client })
    }

    pub fn track(&self, id: &str) -> Result<SpotifyTrackData, SpotifyError> {
        let track_id = TrackId::from_id(id)?;
        let track = self.client.track(track_id)?;

        Ok(SpotifyTrackData::from(track))
    }

    pub fn playlist(&self, id: &str) -> Result<Vec<SpotifyTrackData>, SpotifyError> {
        let playlist_id = PlaylistId::from_id(id)?;
        let playlist = match self.client.playlist(playlist_id, None, None) {
            Ok(playlist) => playlist,
            Err(err) => {
                if let ClientError::Http(http_error) = &err {
                    if let rspotify::http::HttpError::StatusCode(status_code) = http_error.as_ref() {
                        if status_code.status() == 404 {
                            return Err(SpotifyError::PlaylistPrivate);
                        }
                    }
                }
                return Err(err.into());
            }
        };
        let mut tracks = vec![];

        for item in playlist.tracks.items.iter() {
            if let Some(playable_item) = &item.track {
                match playable_item {
                    PlayableItem::Track(track) => {
                        tracks.push(SpotifyTrackData::from(track))
                    },
                    PlayableItem::Episode(_) => {
                        return Err(SpotifyError::EpisodesUnsupported);
                    }
                }
            }
        }
        
        if tracks.is_empty() { return Err(SpotifyError::EmptyPlaylist.into()); }
        Ok(tracks)
    }

    pub fn album(&self, id: &str) -> Result<Vec<SpotifyTrackData>, SpotifyError> {
        let album_id = AlbumId::from_id(id)?;
        let album = match self.client.album(album_id) {
            Ok(album) => album,
            Err(err) => {
                if let ClientError::Http(http_error) = &err {
                    if let rspotify::http::HttpError::StatusCode(status_code) = http_error.as_ref() {
                        if status_code.status() == 404 {
                            return Err(SpotifyError::PlaylistPrivate);
                        }
                    }
                }
                return Err(err.into());
            }
        };
        let mut tracks = vec![];

        for item in album.tracks.items.iter() {
            tracks.push(SpotifyTrackData::from(item));
        }

        if tracks.is_empty() { return Err(SpotifyError::EmptyPlaylist.into()); }
        Ok(tracks)
    }
}