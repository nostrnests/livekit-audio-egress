use std::collections::HashMap;

use anyhow::{Error, Result};
use ffmpeg_rs_raw::ffmpeg_sys_the_third::AVCodecID::{AV_CODEC_ID_AAC, AV_CODEC_ID_AAC_LATM};
use ffmpeg_rs_raw::ffmpeg_sys_the_third::AVSampleFormat::AV_SAMPLE_FMT_S16;
use ffmpeg_rs_raw::ffmpeg_sys_the_third::{av_frame_alloc, av_packet_free, avcodec_find_encoder_by_name};
use ffmpeg_rs_raw::{cstr, Encoder, Muxer};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::speaker::SpeakerChannel;

pub const NB_CHANNELS: u32 = 2;
pub const SAMPLE_RATE: u32 = 48_000;

pub struct Mixer {
    id: String,
    chan_in: UnboundedReceiver<MixerData>,
    encoder: Encoder,
    muxer: Muxer,
    speakers: HashMap<String, SpeakerChannel>,
    pts: i64,
    delay: i64,
}

#[derive(Clone)]
pub struct MixerData {
    pub sid: String,
    pub data: Vec<i16>,
}

impl Mixer {
    pub fn new(id: String, rx: UnboundedReceiver<MixerData>) -> Result<Self> {
        let encoder = unsafe {
            // TODO: this isnt working
            let codec = avcodec_find_encoder_by_name(cstr!("libfdk_aac"));
            Encoder::new(AV_CODEC_ID_AAC)?
                .with_default_channel_layout(NB_CHANNELS as i32)
                .with_sample_rate(SAMPLE_RATE as i32)
                .with_sample_format(AV_SAMPLE_FMT_S16)
                .open(None)?
        };
        let muxer = unsafe {
            let mut opt = HashMap::new();
            opt.insert("hls_flags".to_string(), "delete_segments".to_string());

            Muxer::builder()
                .with_output_path(format!("{}/live.m3u8", id).as_str(), Some("hls"), Some(opt))?
                .with_stream_encoder(&encoder)?
                .build()?
        };

        Ok(Self {
            id,
            chan_in: rx,
            encoder,
            muxer,
            speakers: HashMap::new(),
            pts: 0,
            // audio delay in samples
            delay: (SAMPLE_RATE as f64 * 0.01).ceil() as i64, // 10ms delay
        })
    }

    pub fn run(&mut self) -> Result<(), Error> {
        while let Ok(samples) = self.chan_in.try_recv() {
            if let Some(speaker) = self.speakers.get_mut(&samples.sid) {
                speaker.put(samples);
            } else {
                let sid = samples.sid.clone();
                let mut new_speaker = SpeakerChannel::new(sid.clone());
                new_speaker.put(samples);
                self.speakers.insert(sid, new_speaker);
            }
        }
        self.mix()
    }

    fn mix(&mut self) -> Result<(), Error> {
        let mut speaking = Vec::new();
        let min_samples = unsafe { (*self.encoder.codec_context()).frame_size as i64 };
        let mut out_samples: Vec<i16> = Vec::with_capacity(min_samples as usize);
        let next_pts = self.pts + min_samples;
        if next_pts < self.delay {
            // wait for more data before starting mixer
            self.pts = next_pts;
            return Ok(());
        }
        for (_sid, speaker) in &mut self.speakers {
            if let Some(next) = speaker.next_samples(next_pts) {
                speaking.push(next);
            }
        }
        if speaking.len() == 0 {
            return Ok(());
        }

        let x = 0;
        while x < out_samples.len() {
            let weight = 1f32 / speaking.len() as f32;
            for speaker in &mut speaking {
                out_samples[x] += (speaker[x] as f32 * weight) as i16;
            }
        }

        self.encode_frame(out_samples, next_pts)
    }

    fn encode_frame(&mut self, mut data: Vec<i16>, pts: i64) -> Result<(), Error> {
        unsafe {
            let frame = av_frame_alloc();
            (*frame).extended_data = data.as_mut_ptr() as *mut *mut u8;
            (*frame).sample_rate = SAMPLE_RATE as libc::c_int;
            (*frame).format = AV_SAMPLE_FMT_S16 as libc::c_int;
            (*frame).pts = pts;
            (*frame).nb_samples = data.len() as i32 / NB_CHANNELS as i32;

            for mut pkt in self.encoder.encode_frame(frame)? {
                self.muxer.write_packet(pkt)?;
                if pkt.is_null() {
                    break;
                }
                av_packet_free(&mut pkt);
            }
        }
        Ok(())
    }
}

unsafe impl Sync for Mixer {}

unsafe impl Send for Mixer {}
