# tsq

A toy 8-step sequencer.

## Summary

8-step sequencer that loops a rhythmic pattern, with a kick drum, open and closed hi-hat, and a melodic tone (sine or square) voice. BPM is fixed at 120. Patterns can be randomized at runtime.

## Tracks

- kick
- hi-hat closed
- hi-hat open
- tone (sine or square)

The tone track plays notes from the C major scale (C4–C5), one per step.

## Controls

| Key | Action |
|-----|--------|
| `r` | randomize pattern |
| `q` | quit |

## Build & Run

```bash
$ cargo build
$ cargo test
$ cargo run

╭ tsq ────────────────────────╮
│ kick:      [x] [ ] [ ] [ ] [x] [ ] [ ] [ ]  │
│ hh closed: [x] [x] [x] [x] [x] [x] [x] [x]  │
│ hh open:   [ ] [ ] [ ] [ ] [ ] [ ] [ ] [x]  │
│ tone:      [x] [ ] [x] [ ] [ ] [x] [ ] [x]  │
│                                             │
│ 120 BPM  voice: sine    r randomize  q quit │
╰───────────────────────────╯
```

## License

[MIT](https://github.com/eiri/tsq/blob/main/LICENSE)
