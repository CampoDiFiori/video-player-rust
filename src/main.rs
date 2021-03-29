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
use video_player_rust::decoder::{DecodedFrame, Decoder};

use std::sync::{Arc, Condvar, Mutex};

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
    let mut canvas = window.into_canvas().target_texture().build().unwrap();

    let mut decoder = Decoder::new("/home/dudko/Videos/djanka.mp4");
    let (video_width, video_height) = decoder.get_video_resolution();

    let mut audio_buffer_mutex = Arc::new(Mutex::new(AudioBuffer::new()));

    let texture_creator = canvas.texture_creator();
    let mut texture = texture_creator
        .create_texture_target(
            sdl2::pixels::PixelFormatEnum::IYUV,
            video_width,
            video_height,
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
    let mut ffmpeg_sample_rate = 41000;
    let music_lock = Arc::new((Mutex::new(false), Condvar::new()));
    let music_lock2 = music_lock.clone();

    let write_callback = |stream: &mut soundio::OutStreamWriter| {
        let mut audio_buffer = audio_buffer_mutex.lock().unwrap();

        let frame_count_max =
            std::cmp::min(stream.frame_count_max(), audio_buffer.frames_remaining());
        // println!("Frames to play: {}", frame_count_max);
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

    let start_time = std::time::Instant::now();
    'window_open: loop {
        for (decoded_frame, pst_in_duration) in decoder.frames() {
            match decoded_frame {
                DecodedFrame::Audio(audio_frame) => {
                    let mut audio_buffer = audio_buffer_mutex.lock().unwrap();
                    audio_buffer.add_frame_data(&audio_frame);
                    // println!("Frame sample rate {}", frame.rate());
                    let duration_since_start = start_time.elapsed();
                    let sleep_time = pst_in_duration
                        .checked_sub(duration_since_start)
                        .unwrap_or(std::time::Duration::from_micros(0));

                    // println!("Audio frame sleep time: {:?}", sleep_time);

                    ::std::thread::sleep(sleep_time);
                }
                DecodedFrame::Video(video_frame) => {
                    texture
                        .update_yuv(
                            None,
                            video_frame.data(0),
                            video_frame.plane_width(0) as usize,
                            video_frame.data(1),
                            video_frame.plane_width(1) as usize,
                            video_frame.data(2),
                            video_frame.plane_width(2) as usize,
                        )
                        .unwrap();
                    canvas.copy(&texture, None, None).unwrap();

                    let duration_since_start = start_time.elapsed();
                    let sleep_time = pst_in_duration
                        .checked_sub(duration_since_start)
                        .unwrap_or(std::time::Duration::from_micros(0));

                    // println!("Video frame sleep time: {:?}", sleep_time);

                    ::std::thread::sleep(sleep_time);

                    canvas.present();
                }
            }
        }

        ::std::thread::sleep(std::time::Duration::from_millis(24));
        if should_quit(&mut event_pump) {
            break 'window_open;
        }
    }

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
