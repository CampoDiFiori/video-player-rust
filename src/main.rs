extern crate ffmpeg_next as ffmpeg;
extern crate sdl2;

mod audiobuffer;
use audiobuffer::AudioBuffer;

#[allow(unused_imports)]
use ffmpeg::{codec, filter, format, frame, media};
#[allow(unused_imports)]
use ffmpeg::{rescale, Rescale};
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use std::path::Path;

use std::sync::{Arc, Condvar, Mutex};

#[allow(dead_code)]
struct VideoReader {
    audio_decoder: Option<ffmpeg::decoder::Audio>,
    video_decoder: Option<ffmpeg::decoder::Video>,
    video_stream_idx: usize,
    audio_stream_idx: usize,
}

impl VideoReader {
    pub fn new() -> Self {
        Self {
            audio_decoder: None,
            video_decoder: None,
            audio_stream_idx: 0,
            video_stream_idx: 0,
        }
    }
}

fn main() -> Result<(), ffmpeg::Error> {
    let sdl_context = sdl2::init().unwrap();
    ffmpeg::init().unwrap();

    let video_subsystem = sdl_context.video().unwrap();
    let window = video_subsystem
        .window("Rust Video Player", 960, 540)
        .position_centered()
        .build()
        .unwrap();
    let mut event_pump = sdl_context.event_pump().unwrap();

    let mut context = format::input(&Path::new("/home/dudko/Videos/djanka.mp4")).unwrap();
    let mut vr: VideoReader = VideoReader::new();

    for stream in context.streams() {
        let codec = stream.codec();

        match codec.medium() {
            media::Type::Video => {
                vr.video_decoder = codec.decoder().video().ok();
            }
            media::Type::Audio => {
                vr.audio_decoder = codec.decoder().audio().ok();
            }
            _ => {}
        }
    }

    let mut canvas = window.into_canvas().target_texture().build().unwrap();

    vr.video_stream_idx = context
        .streams()
        .best(media::Type::Video)
        .map(|stream| stream.index())
        .unwrap();

    vr.audio_stream_idx = context
        .streams()
        .best(media::Type::Audio)
        .map(|stream| stream.index())
        .unwrap();

    let mut video_decoder = vr.video_decoder.unwrap();
    let mut audio_decoder = vr.audio_decoder.unwrap();
    let mut helper_video_frame = frame::Video::empty();
    let mut helper_audio_frame = frame::Audio::empty();

    let mut audio_buffer = AudioBuffer::new();

    let texture_creator = canvas.texture_creator();
    let mut texture = texture_creator
        .create_texture_target(
            sdl2::pixels::PixelFormatEnum::IYUV,
            video_decoder.width(),
            video_decoder.height(),
        )
        .unwrap();

    let mut soundioctx = soundio::Context::new();
    soundioctx
        .connect_backend(soundio::Backend::PulseAudio)
        .expect("Backend not supported");

    soundioctx.flush_events();
    let out_dev = soundioctx
        .default_output_device()
        .expect("Does not support this device");
    let mut ffmpeg_sample_rate = 0;
    let music_lock = Arc::new((Mutex::new(false), Condvar::new()));
    let music_lock2 = music_lock.clone();
    'window_open: loop {
        let start_time = std::time::Instant::now();
        for (stream, packet) in context.packets() {
            if stream.index() == vr.video_stream_idx {
                video_decoder.send_packet(&packet)?;
                video_decoder.receive_frame(&mut helper_video_frame)?;

                texture
                    .update_yuv(
                        None,
                        helper_video_frame.data(0),
                        helper_video_frame.plane_width(0) as usize,
                        helper_video_frame.data(1),
                        helper_video_frame.plane_width(1) as usize,
                        helper_video_frame.data(2),
                        helper_video_frame.plane_width(2) as usize,
                    )
                    .unwrap();
                canvas.copy(&texture, None, None).unwrap();

                let frame_pts = std::time::Duration::from_secs_f64(
                    helper_video_frame.pts().unwrap() as f64
                        * stream.time_base().numerator() as f64
                        / stream.time_base().denominator() as f64,
                );
                let duration_since_start = start_time.elapsed();
                let _sleep_time = frame_pts
                    .checked_sub(duration_since_start)
                    .unwrap_or(std::time::Duration::from_micros(0));

                // ::std::thread::sleep(sleep_time);

                canvas.present();
                // println!("Sleep time: {:?}", sleep_time);
            }
            if stream.index() == vr.audio_stream_idx {
                audio_decoder.send_packet(&packet)?;
                audio_decoder.receive_frame(&mut helper_audio_frame)?;

                audio_buffer.add_frame_data(&mut helper_audio_frame);
                ffmpeg_sample_rate = helper_audio_frame.rate() as i32;
                helper_audio_frame.channel_layout();
            }
            if should_quit(&mut event_pump) {
                break 'window_open;
            };
        }
        ::std::thread::sleep(std::time::Duration::from_millis(24));
        if should_quit(&mut event_pump) {
            break 'window_open;
        }
    }

    let write_callback = |stream: &mut soundio::OutStreamWriter| {
        let frame_count_max =
            std::cmp::min(stream.frame_count_max(), audio_buffer.frames_remaining());
        match stream.begin_write(frame_count_max) {
            Ok(_) => {}
            // we reached the end of the buffer
            Err(soundio::Error::Invalid) => {
                let (lock, cvar) = &*music_lock;
                let mut quit_music = lock.lock().unwrap();
                *quit_music = true;
                cvar.notify_one();
                // soundioctx.wakeup();
                return;
            }
            Err(_) => panic!("Something went terribly wrong in write_callback"),
        };
        for f in 0..stream.frame_count() {
            if let Some((lsample, rsample)) = audio_buffer.next() {
                stream.set_sample::<f32>(0, f, lsample);
                stream.set_sample::<f32>(1, f, rsample);
            }
        }
    };

    let closes_sample_rate = out_dev.nearest_sample_rate(ffmpeg_sample_rate);

    let mut outstream = out_dev
        .open_outstream(
            closes_sample_rate,
            soundio::Format::Float32LE,
            soundio::ChannelLayout::get_default(2),
            0.1f64,
            write_callback,
            Some(|| println!("Underflow")),
            Some(|err: soundio::Error| println!("Write callback error: {}", err)),
        )
        .unwrap();

    outstream.start().unwrap();

    let (lock, cvar) = &*music_lock2;
    let mut quit_music = lock.lock().unwrap();
    while !*quit_music {
        // soundioctx.wait_events();
        quit_music = cvar.wait(quit_music).unwrap();
    }

    Ok(())
}

fn should_quit(event_pump: &mut sdl2::EventPump) -> bool {
    for event in event_pump.poll_iter() {
        match event {
            Event::Quit { .. }
            | Event::KeyDown {
                keycode: Some(Keycode::Escape),
                ..
            } => return true,
            _ => {}
        }
    }
    false
}
