# Video Player written in Rust

An attempt to learn about threading in Rust and video processing in general. With this you should be able to play an mp4 video with sound on Windows, Linux or Mac.

## Used libraries:

- ffmpeg (for decoding frames)
- SDL2 (for presenting video frames)
- libsoundio (for presenting audio frames)

## TODO:

- make the libsoundio `write` callback lock-free
- add support for skipping and stopping the playback
