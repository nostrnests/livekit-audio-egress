use anyhow::Error;
use futures_util::StreamExt;
use livekit::prelude::{RemoteAudioTrack, RemoteTrack};
use livekit::webrtc::audio_stream::native::NativeAudioStream;
use livekit::webrtc::native::audio_resampler;
use livekit::{Room, RoomEvent, RoomOptions};
use log::info;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

use crate::mixer::{Mixer, MixerData, NB_CHANNELS, SAMPLE_RATE};

#[derive(Serialize, Deserialize)]
struct GetTokenResponse {
    pub token: String,
}

pub struct Egress {
    room: String,
}

impl Egress {
    pub fn new(room: String) -> Self {
        Self { room }
    }

    pub async fn run(&self) -> Result<(), Error> {
        let url = "wss://nostrnests.com";
        let guest_auth_url = format!("https://nostrnests.com/api/v1/nests/{}/guest", self.room);
        let token: GetTokenResponse = reqwest::get(guest_auth_url).await?.json().await?;

        let (room, mut rx) = Room::connect(&url, &token.token, RoomOptions::default()).await?;
        info!("Connected to room: {} - {}", room.name(), room.sid().await);

        let (dtx, drx) = unbounded_channel();
        let mut mixer = Mixer::new(room.name(), drx)?;
        tokio::task::spawn_blocking(move || {
            mixer.run().expect("Mixer failed");
        });

        while let Some(msg) = rx.recv().await {
            match msg {
                RoomEvent::TrackSubscribed {
                    track,
                    publication: _,
                    participant: px,
                } => {
                    if let RemoteTrack::Audio(audio_track) = track {
                        info!("{} became speaker in {}", px.identity(), &self.room);
                        tokio::spawn(Self::record_track(audio_track, dtx.clone()));
                        break;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    async fn record_track(
        audio_track: RemoteAudioTrack,
        to_mixer: UnboundedSender<MixerData>,
    ) -> Result<(), Error> {
        let rtc_track = audio_track.rtc_track();
        let mut resampler = audio_resampler::AudioResampler::default();
        let mut audio_stream =
            NativeAudioStream::new(rtc_track, SAMPLE_RATE as i32, NB_CHANNELS as i32);
        while let Some(frame) = audio_stream.next().await {
            let data = resampler.remix_and_resample(
                &frame.data,
                frame.samples_per_channel,
                frame.num_channels,
                frame.sample_rate,
                NB_CHANNELS,
                SAMPLE_RATE,
            );
            to_mixer.send(MixerData {
                sid: audio_track.sid().to_string(),
                data: Vec::from(data),
            })?;
        }
        Ok(())
    }
}
