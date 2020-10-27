use std::convert::TryInto;

pub struct AudioBuffer {
    lch: Vec<u8>,
    rch: Vec<u8>,
    psts: Vec<i64>,
    read_idx: usize,
}

const SAMPLE_SIZE: usize = 4;

impl AudioBuffer {
    pub fn new() -> Self {
        Self {
            lch: Vec::new(),
            rch: Vec::new(),
            psts: Vec::new(),
            read_idx: 0,
        }
    }

    pub fn frames_remaining(&self) -> usize {
        (self.lch.len() - self.read_idx) / SAMPLE_SIZE
    }

    pub fn add_frame_data(&mut self, ffmpeg_frame: &ffmpeg::frame::Audio) {
        let len = ffmpeg_frame.samples() * SAMPLE_SIZE;

        self.lch.extend_from_slice(&ffmpeg_frame.data(0)[0..len]);
        self.rch.extend_from_slice(&ffmpeg_frame.data(0)[0..len]);
        self.psts.push(ffmpeg_frame.pts().unwrap());
    }
}

impl Iterator for AudioBuffer {
    type Item = (f32, f32);

    fn next(&mut self) -> Option<Self::Item> {
        if self.lch.len() < self.read_idx + SAMPLE_SIZE {
            None
        } else {
            let lbytes: [u8; SAMPLE_SIZE] = self.lch[self.read_idx..(self.read_idx + SAMPLE_SIZE)]
                .try_into()
                .unwrap();
            let rbytes: [u8; SAMPLE_SIZE] = self.rch[self.read_idx..(self.read_idx + SAMPLE_SIZE)]
                .try_into()
                .unwrap();

            let lsample = std::primitive::f32::from_ne_bytes(lbytes);
            let rsample = std::primitive::f32::from_ne_bytes(rbytes);

            self.read_idx += SAMPLE_SIZE;

            Some((lsample, rsample))
        }
    }
}
