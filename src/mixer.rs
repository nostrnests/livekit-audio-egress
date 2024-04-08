use std::ptr;
use anyhow::Error;
use ffmpeg_sys_next::{AV_CH_LAYOUT_STEREO, av_dump_format, av_opt_set, AVChannelLayout, AVChannelLayout__bindgen_ty_1, AVCodecContext, avfilter_get_by_name, avfilter_graph_alloc, avfilter_graph_alloc_filter, AVFilterContext, AVFilterGraph, avformat_alloc_context, avformat_alloc_output_context2, avformat_new_stream, AVFormatContext};
use ffmpeg_sys_next::AVChannelOrder::AV_CHANNEL_ORDER_NATIVE;
use ffmpeg_sys_next::AVCodecID::AV_CODEC_ID_AAC;
use ffmpeg_sys_next::AVMediaType::AVMEDIA_TYPE_AUDIO;
use ffmpeg_sys_next::AVSampleFormat::AV_SAMPLE_FMT_FLTP;
use tokio::sync::mpsc::UnboundedReceiver;

pub struct Mixer {
    id: String,
    chan_in: UnboundedReceiver<MixerData>,
    ctx: *mut AVFormatContext,
    flt_ctx: *mut AVFilterContext,
    flt_graph: *mut AVFilterGraph,
}

#[derive(Clone)]
pub struct MixerData {
    pub sid: String,
    pub data: Vec<i16>,
}

impl Mixer {
    pub fn new(id: String, rx: UnboundedReceiver<MixerData>) -> Self {
        Self { id, chan_in: rx, ctx: ptr::null_mut(), flt_ctx: ptr::null_mut(), flt_graph: ptr::null_mut() }
    }

    fn setup_mixer(&mut self) -> Result<(), Error> {
        unsafe {
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
            let params = (*stream).codecpar;

            (*params).codec_id = AV_CODEC_ID_AAC;
            (*params).codec_type = AVMEDIA_TYPE_AUDIO;
            (*params).format = AV_SAMPLE_FMT_FLTP as libc::c_int;
            (*params).bit_rate = 320_000;
            (*params).sample_rate = 48_000 as libc::c_int;
            (*params).ch_layout = AVChannelLayout {
                order: AV_CHANNEL_ORDER_NATIVE,
                nb_channels: 2,
                u: AVChannelLayout__bindgen_ty_1 {
                    mask: AV_CH_LAYOUT_STEREO,
                },
                opaque: ptr::null_mut(),
            };
            av_dump_format(ctx, 0, ptr::null(), 1);

            let mut flt_ctx = avfilter_graph_alloc();

            let abuf = avfilter_get_by_name("abuffer\0".as_ptr() as *const libc::c_char);
            avfilter_graph_alloc_filter(flt_ctx, abuf, "src\0".as_ptr() as *const libc::c_char);

            self.ctx = ctx;
        }
        Ok(())
    }
    pub fn run(&mut self) -> Result<(), Error> {
        if self.ctx == ptr::null_mut() {
            self.setup_mixer()?;
        }
        Ok(())
    }
}

unsafe impl Sync for Mixer {}

unsafe impl Send for Mixer {}