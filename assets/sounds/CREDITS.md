# Sound Credits

All files in this directory are **placeholders**. They were synthesized procedurally
(short filtered-noise bursts with a damped resonant tone) rather than recorded, so no
third-party recording is redistributed here. Replace them with real CC0 recordings if
you want authentic switch sounds.

Format of every file: mono, 44.1 kHz, 16-bit PCM WAV, shorter than 150 ms.

## Placeholder recipes

The placeholders were produced with the equivalent of these `sox` commands:

```
# mx-blue press (sharp, high click)
sox -n -r 44100 -c 1 -b 16 press.wav synth 0.025 pinknoise fade 0 0.025 0.02 gain -6

# space bar (deeper, slightly longer)
sox -n -r 44100 -c 1 -b 16 space.wav synth 0.04 brownnoise fade 0 0.04 0.03 gain -6
```

`mx-red`, `mx-brown` and `topre` use the same shape with a lower noise cutoff and a
lower resonant tone, giving progressively deeper "thock".

## Replacing with real recordings

If you swap in recordings, keep them CC0 (public domain) and record for each file:

| File | Source URL | License | Author |
| ---- | ---------- | ------- | ------ |
| _none yet_ | | | |
