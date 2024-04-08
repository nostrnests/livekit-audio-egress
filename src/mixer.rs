use std::collections::HashMap;
use std::ptr;

use anyhow::Error;
use ffmpeg_sys_next::{av_channel_layout_copy, av_channel_layout_default, av_dump_format, av_frame_alloc, av_interleaved_write_frame, av_opt_set, av_packet_alloc, av_packet_free, avcodec_alloc_context3, avcodec_find_encoder, avcodec_find_encoder_by_name, avcodec_parameters_from_context, avcodec_receive_packet, avcodec_send_frame, AVCodecContext, AVERROR, AVERROR_EOF, avformat_alloc_output_context2, avformat_new_stream, avformat_write_header, AVFormatContext};
use ffmpeg_sys_next::AVCodecID::AV_CODEC_ID_AAC;
use ffmpeg_sys_next::AVSampleFormat::AV_SAMPLE_FMT_S16;
use libc::EAGAIN;
use tokio::sync::mpsc::UnboundedReceiver;

use crate::speaker::SpeakerChannel;

pub const NB_CHANNELS: u32 = 2;
pub const SAMPLE_RATE: u32 = 48_000;

pub struct Mixer {
    id: String,
    chan_in: UnboundedReceiver<MixerData>,
    ctx: *mut AVFormatContext,
    codec_ctx: *mut AVCodecContext,
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
    pub fn new(id: String, rx: UnboundedReceiver<MixerData>) -> Self {
        Self {
            id,
            chan_in: rx,
            ctx: ptr::null_mut(),
            codec_ctx: ptr::null_mut(),
            speakers: HashMap::new(),
            pts: 0,
            // audio delay in samples
            delay: (SAMPLE_RATE as f64 * 0.01).ceil() as i64 // 10ms delay
        }
    }

    fn setup_mixer(&mut self) -> Result<(), Error> {
        unsafe {
            let codec = avcodec_find_encoder_by_name("libfdk_aac\0".as_ptr() as *const libc::c_char);
            if codec.is_null() {
                return Err(Error::msg("Could not find encoder"));
            }

            let codec_ctx = avcodec_alloc_context3(codec);
            if codec_ctx.is_null() {
                return Err(Error::msg("Could not find encoder"));
            }

            (*codec_ctx).sample_rate = SAMPLE_RATE as libc::c_int;
            (*codec_ctx).sample_fmt = AV_SAMPLE_FMT_S16;
            av_channel_layout_default(&mut (*codec_ctx).ch_layout, NB_CHANNELS as libc::c_int);

            let mut ctx = ptr::null_mut();
            let ret = avformat_alloc_output_context2(
                &mut ctx,
                ptr::null(),
                "hls\0".as_ptr() as *const libc::c_char,
                format!("{}/live.m3u8\0", self.id).as_ptr() as *const libc::c_char,
            );
            if ret < 0 {
                return Err(Error::msg("Mixer failed to init"));
            }

            av_opt_set(
                (*ctx).priv_data,
                "hls_flags\0".as_ptr() as *const libc::c_char,
                "delete_segments\0".as_ptr() as *const libc::c_char,
                0,
            );

            let stream = avformat_new_stream(ctx, ptr::null());
            if stream == ptr::null_mut() {
                return Err(Error::msg("Failed to add stream to output"));
            }
            avcodec_parameters_from_context((*stream).codecpar, codec_ctx);

            av_dump_format(ctx, 0, ptr::null(), 1);

            let ret = avformat_write_header(ctx, ptr::null_mut());
            if ret < 0 {
                return Err(Error::msg("Failed to write header"));
            }
            self.codec_ctx = codec_ctx;
            self.ctx = ctx;
        }
        Ok(())
    }

    pub fn run(&mut self) -> Result<(), Error> {
        if self.ctx == ptr::null_mut() {
            self.setup_mixer()?;
        }
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
        let min_samples = self.frame_size();
        let mut out_samples: Vec<i16> = Vec::with_capacity(min_samples as usize);
        let next_pts = self.pts + min_samples;
        if next_pts < self.delay {
            // wait for more data before starting mixer
            self.pts = next_pts;
            return Ok(())
        }
        for (sid, mut speaker) in &mut self.speakers {
            if let Some(next) = speaker.next_samples(next_pts) {
                speaking.push(next);
            }
        }
        if speaking.len() == 0 {
            return Ok(())
        }

        let x =0;
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
            av_channel_layout_copy(&mut (*frame).ch_layout, &(*self.codec_ctx).ch_layout);


            let mut ret = avcodec_send_frame(self.codec_ctx, frame);
            if ret < 0 {
                return Err(Error::msg("Failed to encode frame"));
            }
            while ret > 0 {
                let mut pkt = av_packet_alloc();
                ret = avcodec_receive_packet(self.codec_ctx, pkt);
                if ret == AVERROR(EAGAIN) {
                    av_packet_free(&mut pkt);
                    return Ok(())
                }
                if ret == AVERROR_EOF {
                    return Err(Error::msg("Stream ended"))
                }

                ret = av_interleaved_write_frame(self.ctx, pkt);
                if ret < 0 {
                    return Err(Error::msg("Failed to write pkt"))
                }
            }
        }
        Ok(())
    }

    fn frame_size(&mut self) -> i64 {
        unsafe {
            (*self.codec_ctx).frame_size as i64
        }
    }
}

unsafe impl Sync for Mixer {}

unsafe impl Send for Mixer {}
