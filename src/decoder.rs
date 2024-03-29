extern crate ffmpeg_next as ffmpeg;
use std::{path::Path, sync::{Arc, Mutex}, time::{Duration, Instant}};

#[allow(unused_imports)]
use ffmpeg::{codec, filter, format, frame, media};

#[derive(Clone, Copy)]
struct StreamsData {
    video_stream_frame_len: f64,
    video_stream_idx: usize,
    audio_stream_frame_len: f64,
    audio_stream_idx: usize,
}

#[allow(dead_code)]
pub struct Decoder {
    input: ffmpeg::format::context::Input,
    audio_decoder: ffmpeg::decoder::Audio,
    video_decoder: ffmpeg::decoder::Video,
    streams_data: StreamsData,
}

#[derive(Clone)]
pub struct PacketBuffer {
    audio_packets: Vec<ffmpeg::Packet>,
    video_packets: Vec<ffmpeg::Packet>,
}

pub struct FrameIter<'a> {
    decoder: &'a mut Decoder,
    packet_buffer: PacketBuffer,
    audio_buffer_mutex: Arc<Mutex<AudioBuffer>>,
    audio_packet_idx: usize,
    video_packet_idx: usize,
}

impl Decoder {
    pub fn new(path: &str) -> Decoder {
        let input = format::input(&Path::new(path)).unwrap();

        let mut video_decoder = None;
        let mut audio_decoder = None;

        for stream in input.streams() {
            let codec = stream.codec();

            match codec.medium() {
                media::Type::Video => {
                    video_decoder = codec.decoder().video().ok();
                }
                media::Type::Audio => {
                    audio_decoder = codec.decoder().audio().ok();
                }
                _ => {}
            }
        }

        let video_stream = input.streams().best(media::Type::Video).unwrap();
        let video_stream_idx = video_stream.index();
        let video_stream_frame_len = video_stream.time_base().numerator() as f64
            / video_stream.time_base().denominator() as f64;

        let audio_stream = input.streams().best(media::Type::Audio).unwrap();
        let audio_stream_idx = audio_stream.index();
        let audio_stream_frame_len = audio_stream.time_base().numerator() as f64
            / audio_stream.time_base().denominator() as f64;

        Self {
            input,
            audio_decoder: audio_decoder.unwrap(),
            video_decoder: video_decoder.unwrap(),
            streams_data: StreamsData {
                video_stream_idx,
                audio_stream_idx,
                audio_stream_frame_len,
                video_stream_frame_len,
            },
        }
    }

    pub fn next_packet_buffer(&mut self) -> PacketBuffer {
        let mut audio_packets = Vec::new();
        let mut video_packets = Vec::new();

        let video_stream_idx = self.streams_data.video_stream_idx;
        let audio_stream_idx = self.streams_data.audio_stream_idx;

        self.input.packets().into_iter().for_each(|(stream, packet)| {
            if stream.index() == video_stream_idx {
                video_packets.push(packet);
            } else if stream.index() == audio_stream_idx {
                audio_packets.push(packet);
            }
        });

        // #[derive(PartialEq)]
        // enum PrevPacket {
        //     None,
        //     Audio,
        //     Video,
        // }

        // let mut prev_packet = PrevPacket::None;
        // let mut reading_state = 0u8;

        // for (stream, packet) in self.input.packets() {
        //     if stream.index() == self.streams_data.video_stream_idx {
        //         video_packets.push(packet);

        //         if prev_packet != PrevPacket::Video {
        //             reading_state = reading_state + 1;
        //         }
        //         if reading_state == 3 {
        //             break;
        //         }
        //         prev_packet = PrevPacket::Video;
        //     } else if stream.index() == self.streams_data.audio_stream_idx {
        //         audio_packets.push(packet);

        //         if prev_packet != PrevPacket::Audio {
        //             reading_state = reading_state + 1;
        //         }
        //         if reading_state == 3 {
        //             break;
        //         }
        //         prev_packet = PrevPacket::Audio;
        //     }
        // }

        PacketBuffer {
            audio_packets,
            video_packets,
        }
    }

    pub fn frames<'a>(&'a mut self, audio_buffer_mutex: Arc<Mutex<AudioBuffer>>) -> FrameIter<'a> {
        let packet_buffer = self.next_packet_buffer();

        // println!("Packet iteration duration: {:?}", packet_iter_end);

        FrameIter {
            packet_buffer,
            audio_buffer_mutex,
            decoder: self,
            audio_packet_idx: 0,
            video_packet_idx: 0,
        }
    }

    pub fn get_video_resolution(&self) -> (u32, u32) {
        (self.video_decoder.width(), self.video_decoder.height())
    }
}

fn calc_video_frame_pts(streams_data: StreamsData, video_frame: &ffmpeg::frame::Video) -> Duration {
    std::time::Duration::from_secs_f64(
        video_frame.pts().unwrap() as f64 * streams_data.video_stream_frame_len,
    )
}

fn calc_audio_frame_pts(streams_data: StreamsData, audio_frame_pts: f64) -> Duration {
    std::time::Duration::from_secs_f64(
        audio_frame_pts * streams_data.audio_stream_frame_len,
    )
}

use ffmpeg::frame::{Audio, Video};

use crate::audiobuffer::AudioBuffer;

pub enum DecodedFrame {
    Audio,
    Video(Video),
}

const ITER_SIZE: usize = 1000;


fn decode_next_audio_packet(frame_iter: &mut FrameIter) -> Option<(DecodedFrame, Duration)> {
    let packet_buffer = frame_iter.packet_buffer.clone();
    let mut first_packet_pts = None;
    let curr_idx = frame_iter.audio_packet_idx;

    packet_buffer.audio_packets[curr_idx..].iter().take(ITER_SIZE).for_each(|packet| {
        let mut audio_frame = ffmpeg::frame::Audio::empty();
        if first_packet_pts == None {
            first_packet_pts = Some(packet.pts().unwrap() as f64);
        }
        frame_iter
            .decoder
            .audio_decoder
            .send_packet(packet)
            .unwrap();
        frame_iter
            .decoder
            .audio_decoder
            .receive_frame(&mut audio_frame)
            .unwrap();
        
        frame_iter.audio_buffer_mutex.lock().unwrap().add_frame_data(&audio_frame);
    });

    let pts = calc_audio_frame_pts(frame_iter.decoder.streams_data, first_packet_pts.unwrap());

    frame_iter.audio_packet_idx = frame_iter.audio_packet_idx + ITER_SIZE;

    Some((DecodedFrame::Audio, pts))
}

fn decode_next_video_packet(frame_iter: &mut FrameIter) -> Option<(DecodedFrame, Duration)> {
    let next_video_packet = &frame_iter.packet_buffer.video_packets[frame_iter.video_packet_idx];
    let mut video_frame = ffmpeg::frame::Video::empty();
    frame_iter
        .decoder
        .video_decoder
        .send_packet(next_video_packet)
        .unwrap();
    frame_iter
        .decoder
        .video_decoder
        .receive_frame(&mut video_frame)
        .unwrap();

    let pts = calc_video_frame_pts(frame_iter.decoder.streams_data, &video_frame);

    frame_iter.video_packet_idx = frame_iter.video_packet_idx + 1;

    Some((DecodedFrame::Video(video_frame), pts))
}

impl<'a> Iterator for FrameIter<'a> {
    type Item = (DecodedFrame, std::time::Duration);

    fn next(&mut self) -> Option<Self::Item> {
        match (
            self.audio_packet_idx < self.packet_buffer.audio_packets.len(),
            self.video_packet_idx < self.packet_buffer.video_packets.len(),
        ) {
            (true, true) => {
                let next_audio_packet = &self.packet_buffer.audio_packets[self.audio_packet_idx];
                let next_video_packet = &self.packet_buffer.video_packets[self.video_packet_idx];

                if next_audio_packet.pts() < next_video_packet.pts() {
                    decode_next_audio_packet(self)
                } else {
                    decode_next_video_packet(self)
                }
            }
            (true, false) => {
                decode_next_audio_packet(self)
            }
            (false, true) => {
                decode_next_video_packet(self)
            }
            _ => None,
        }
    }
}
