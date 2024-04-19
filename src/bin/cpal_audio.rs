use audio_example::{audio::sample_rate::SampleRateConverter, log_conf::init_tracing_subscriber};
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    SampleRate,
};
use crossbeam::channel::Sender;
use ffmpeg::{format, media};
use ffmpeg_next::{self as ffmpeg, ChannelLayout};

type AudioFormatT = i16;

fn main() {
    init_tracing_subscriber(&["cpal_audio"]);
    log_panics::init();
    let host = cpal::default_host();
    // let devices = host.devices().unwrap();
    let device = host.default_output_device().unwrap();
    let mut supported_configs_range = device.supported_output_configs().unwrap();
    let config = supported_configs_range
        .next()
        .unwrap()
        .with_max_sample_rate();
    log::info!(
        "rate = {:?}, format = {:?}, channels = {}, buffer size = {:?}",
        config.sample_rate(),
        config.sample_format(),
        config.channels(),
        config.buffer_size()
    );
    let mut frame_buf: Vec<AudioFormatT> = Vec::with_capacity(8192);
    let (tx, rx) = crossbeam::channel::bounded(2);
    let rate = config.sample_rate();
    let j = std::thread::spawn(move || {
        init_ffmpeg(tx, rate.0).unwrap();
    });
    let stream = device
        .build_output_stream(
            &config.config(),
            move |data: &mut [AudioFormatT], _info| {
                if frame_buf.len() < data.len() {
                    while let Ok(buf) = rx.try_recv() {
                        frame_buf.extend(&buf);
                    }
                }
                // log::debug!("{}", frame_buf.len());
                let len = frame_buf.len().min(data.len());
                frame_buf
                    .drain(..len)
                    .zip(data[..len].iter_mut())
                    .for_each(|(v, d)| {
                        *d = v;
                    });
            },
            |err| {
                log::error!("stream error {err:?}");
            },
            None,
        )
        .unwrap();
    stream.play().unwrap();
    j.join().unwrap();
}

fn init_ffmpeg(tx: Sender<Vec<AudioFormatT>>, rate: u32) -> Result<(), ffmpeg::Error> {
    let mut args = std::env::args();
    args.next();
    let Some(music) = args.next() else {
        log::error!("请传入音频文件参数");
        std::process::exit(1);
    };
    let mut input_context = ffmpeg::format::input(&music)?;
    let audio_input = input_context
        .streams()
        .best(media::Type::Audio)
        .ok_or(ffmpeg::Error::StreamNotFound)?;
    let stream_index = audio_input.index();
    let context_decoder =
        ffmpeg::codec::context::Context::from_parameters(audio_input.parameters())?;
    let mut decoder = context_decoder.decoder().audio()?;
    let channels = decoder.channels();
    let mut audio_frame = ffmpeg::frame::Audio::empty();
    let mut audio_convert_frame = ffmpeg::frame::Audio::empty();
    for (stream, packet) in input_context.packets() {
        if stream.index() == stream_index {
            decoder.send_packet(&packet)?;
            while decoder.receive_frame(&mut audio_frame).is_ok() {
                let mut sample_convert = audio_frame.resampler(
                    format::Sample::I16(format::sample::Type::Packed),
                    ChannelLayout::STEREO,
                    audio_frame.rate(),
                )?;
                sample_convert.run(&audio_frame, &mut audio_convert_frame)?;
                // log::info!(
                //     "{} rate = {} samples = {}",
                //     audio_convert_frame.data(0).len(),
                //     rate,
                //     audio_convert_frame.samples()
                // );
                let pcm_samples = audio_convert_frame.data(0).chunks(2).map(|buf| {
                    (AudioFormatT::from_le_bytes(buf.try_into().unwrap()) as f32) as AudioFormatT
                });
                let convert_sample = SampleRateConverter::new(
                    pcm_samples,
                    SampleRate(audio_convert_frame.rate()),
                    SampleRate(rate),
                    channels,
                );
                tx.send(convert_sample.collect()).unwrap();
            }
        }
    }
    Ok(())
}
