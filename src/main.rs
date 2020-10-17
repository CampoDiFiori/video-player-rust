extern crate ffmpeg_next as ffmpeg;
extern crate sdl2;

use ffmpeg::format::Pixel;
use ffmpeg::software::scaling::{context::Context, flag::Flags};
#[allow(unused_imports)]
use ffmpeg::{codec, filter, format, frame, media};
#[allow(unused_imports)]
use ffmpeg::{rescale, Rescale};
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::surface::Surface;
use std::path::Path;

#[allow(dead_code)]
struct VideoReader {
    audio_decoder: Option<ffmpeg::decoder::Audio>,
    video_decoder: Option<ffmpeg::decoder::Video>,
    video_stream_idx: Option<usize>,
    audio_stream_idx: Option<usize>,
}

impl VideoReader {
    pub fn new() -> Self {
        Self {
            audio_decoder: None,
            video_decoder: None,
            audio_stream_idx: None,
            video_stream_idx: None,
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

    vr.video_stream_idx = context
        .streams()
        .best(media::Type::Video)
        .map(|stream| stream.index());

    let mut video_decoder = vr.video_decoder.unwrap();
    let mut helper_video_frame = frame::Video::empty();
    let mut helper_rgb_video_frame = frame::Video::empty();
    let mut scaler = Context::get(
        video_decoder.format(),
        video_decoder.width(),
        video_decoder.height(),
        Pixel::RGB24,
        video_decoder.width(),
        video_decoder.height(),
        Flags::BILINEAR,
    )?;

    'window_open: loop {
        for (stream, packet) in context.packets() {
            if stream.index() == vr.video_stream_idx.unwrap() {
                let mut window_surface = window.surface(&event_pump).unwrap();
                video_decoder.send_packet(&packet)?;
                video_decoder.receive_frame(&mut helper_video_frame)?;
                scaler.run(&helper_video_frame, &mut helper_rgb_video_frame)?;

                Surface::from_data(
                    helper_rgb_video_frame.data_mut(0),
                    video_decoder.width(),
                    video_decoder.height(),
                    video_decoder.width() * 3,
                    sdl2::pixels::PixelFormatEnum::RGB24,
                )
                .unwrap()
                .blit_scaled(None, &mut window_surface, None)
                .unwrap();
                window_surface.update_window().unwrap();

                if should_quit(&mut event_pump) {
                    break 'window_open;
                };
            }
        }
        if should_quit(&mut event_pump) {
            break 'window_open;
        }
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
