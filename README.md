# tsq

[![CI](https://github.com/eiri/tsq/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/eiri/tsq/actions/workflows/ci.yml)
[![License](https://img.shields.io/github/license/eiri/tsq)](LICENSE)

A toy 8-step music sequencer that uses emulated YM2149 sound chip.

The pattern plays at 120 beats per minute. It has four tracks:

- Melody: notes from C4 to C5, with two sound variations (M1 and M2).
- Arpeggio: a group of notes played one after another with a different voice.
- Bass: low notes with a rounded sound.
- Drums: kick, snare, and open or closed hi-hat.

Bass and drums share one chip channel, so a drum can interrupt a bass note.

## Controls

| Key     | Action                    |
| ------- | ------------------------- |
| `p`     | Start or stop playback    |
| `t`     | Move the track marker     |
| `r`     | Randomize selected track  |
| `CMD+Q` | Quit                      |

## Build & Run

```bash
$ cargo build
$ cargo test
$ cargo run
```

## License

[MIT](LICENSE)
