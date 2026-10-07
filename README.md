# tsq

[![CI](https://github.com/eiri/tsq/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/eiri/tsq/actions/workflows/ci.yml)
[![License](https://img.shields.io/github/license/eiri/tsq)](LICENSE)

A toy 8-step sequencer.

![tsq UI](ui.png)

## Summary

Two emulated YM2149 sound chips. 8-step sequencer loops a pattern of kick, snare, hi-hat, and melody. The tempo is fixed at 120 BPM and you can randomize the pattern during playback.

## Tracks

- kick
- snare
- open/closed hi-hat
- melody (pluck or sustain)

The melody plays one C major note per step, from C4 to C5.

## Controls

| Key     | Action                  |
| ------- | ----------------------- |
| `p`     | start/stop play         |
| `t`     | choose instrument track |
| `r`     | randomize pattern       |
| `CMD+Q` | quit                    |

## Build & Run

```bash
$ cargo build
$ cargo test
$ cargo run
```

## License

[MIT](LICENSE)
