use std::ffi::CStr;

use anyhow::Error;
use futures_util::stream::StreamExt;
use livekit::{Room, RoomEvent, RoomOptions};
use livekit::track::{RemoteAudioTrack, RemoteTrack};
use livekit::webrtc::audio_stream::native::NativeAudioStream;
use livekit::webrtc::native::audio_resampler;
use log::{info, warn};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

use crate::egress::Egress;
use crate::mixer::{Mixer, MixerData, NB_CHANNELS, SAMPLE_RATE};

mod mixer;
mod speaker;
mod egress;

#[tokio::main]
async fn main() -> Result<(), Error> {
    pretty_env_logger::init();

    unsafe {
        info!("FFMPEG version={}",
            CStr::from_ptr(ffmpeg_sys_next::av_version_info()).to_str()?);
    }

    let mut rooms = Vec::new();
    for r in vec![
        "427769e1-9679-4113-b46b-33d947a38609".to_string()
    ] {
        rooms.push(tokio::spawn(async move {
            let egress = Egress::new(r);
            if let Err(e) = egress.run().await {
                warn!("Error running egress {}", e);
            }
        }));
    }

    for j in rooms {
        j.await.expect("TODO: panic message");
    }
    Ok(())
}