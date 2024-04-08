mod mixer;

use crate::mixer::{Mixer, MixerData};
use anyhow::Error;
use livekit::{Room, RoomEvent, RoomOptions};
use livekit::track::{RemoteAudioTrack, RemoteTrack};
use livekit::webrtc::audio_stream::native::NativeAudioStream;
use livekit::webrtc::native::audio_resampler;
use futures_util::stream::StreamExt;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

#[tokio::main]
async fn main() {
    let url = "wss://nostrnests.com";
    let token = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJleHAiOjE3MTEzMjExNDAsImlzcyI6IkFQSWNxNUJwbVZNSmhEVSIsInN1YiI6Imd1ZXN0LWM5Y2FjOWVjLTEzMjAtNDVjOS05ZjNlLTU0NGY2OWQzYjcxMyIsIm5iZiI6MTcxMTMyMDU0MCwidmlkZW8iOnsicm9vbSI6IjQyNzc2OWUxLTk2NzktNDExMy1iNDZiLTMzZDk0N2EzODYwOSIsInJvb21Kb2luIjp0cnVlLCJjYW5QdWJsaXNoIjpmYWxzZSwiY2FuU3Vic2NyaWJlIjp0cnVlfX0.FcLUx6_26vGNvoS9bUZnEAUdvDDI4sVTPkvboyVeHTQ";

    let (room, mut rx) = Room::connect(&url, &token, RoomOptions::default())
        .await
        .unwrap();
    println!("Connected to room: {} - {}", room.name(), room.sid());


    let (dtx, drx) = unbounded_channel();
    let mut mixer = Mixer::new(room.name(), drx);
    tokio::task::spawn_blocking(move || {
        mixer.run().expect("Mixer failed");
    });

    while let Some(msg) = rx.recv().await {
        match msg {
            RoomEvent::TrackSubscribed {
                track,
                publication: _,
                participant: _,
            } => {
                if let RemoteTrack::Audio(audio_track) = track {
                    tokio::spawn(record_track(audio_track, dtx.clone()));
                    break;
                }
            }
            _ => {}
        }
    }

    println!("Done");
}

async fn record_track(audio_track: RemoteAudioTrack, to_mixer: UnboundedSender<MixerData>) -> Result<(), Error> {
    println!("Recording track {:?}", audio_track.sid());
    let rtc_track = audio_track.rtc_track();
    let mut resampler = audio_resampler::AudioResampler::default();
    let mut audio_stream = NativeAudioStream::new(rtc_track);
    while let Some(frame) = audio_stream.next().await {
        let data = resampler.remix_and_resample(
            &frame.data,
            frame.samples_per_channel,
            frame.num_channels,
            frame.sample_rate,
            2,
            48_000,
        );
        to_mixer.send(MixerData {
            sid: audio_track.sid().to_string(),
            data: Vec::from(data),
        }).unwrap();
    }
    Ok(())
}