# Record offset

> **Last reviewed:** 2026-10-30 · **Last updated:** 2026-10-30 · **Change:** minor (created)

What you hear is not what is recorded at the same moment. The microphone signal goes through the
interface, its driver and a buffer before SoundCraft gets it, and what you play along to takes a
similar way out to your headphones. A take recorded at the playhead therefore lands behind the
music it was played to, by the round-trip latency. The **record offset** places every new
recording that many samples earlier, so it lines up again.

## What it does

- A new recording is placed `offset` samples earlier than where the transport started. When that
  would be before the start of the session, the clip starts at 0 and the first part of the take is
  cut off (nothing is lost after it).
- It applies to every new recording: plain takes, **loop recording** (each pass keeps its place in
  the loop and starts `offset` samples into its part of the take, so the passes do not overlap)
  and **punch** (the punch range is cut from the shifted take).
- The offset is fixed when a recording starts, and saved with the recording's plan, so a crash
  recovery places the take the same way. Existing clips are never moved.

## Where the offset comes from

The offset is the sum of two parts:

1. **Manual value**, in milliseconds or samples. Default 0. It is kept with the preferences, so it
   belongs to this computer and audio interface, not to the session.
2. **Driver-reported latency**, only if *Also add the latency the audio driver reports* is on
   (default **off**). SoundCraft measures it in the audio callbacks: for the input, the time from
   capture to the callback; for the output, from the callback to playback.

**Why the manual value stays necessary:** many drivers do not know, or do not report, the real
latency. WASAPI in shared mode and USB audio class devices typically leave out the converter and
USB buffering, so the reported value is too small or 0. Core Audio and ASIO usually report better.
If you switch the automatic part on, check the result with the clap test below.

## Setting it

- **Window:** Setup › Playback Engine (or Setup › Record Offset…), section *Record offset*.
- **Command:** `setup.record_offset`, the same id from the menu, the CLI and the control channel.

  ```json
  {"command": "setup.record_offset", "params": {"ms": 12.5}}
  {"command": "setup.record_offset", "params": {"samples": 600, "auto": true}}
  {"command": "setup.record_offset", "params": {"reset": true}}
  ```

  `ms` (0 to 5000) and `samples` (0 to 1,000,000) are alternatives. `auto` switches the reported
  part on or off. The result lists `manual_samples`, `reported_samples` and `total_samples` at the
  session's sample rate. Invalid values are refused and change nothing. Headless (no desktop app)
  the setting is kept in the session and nothing is reported by a driver.

## Calibrating with a clap test

1. Set the offset to 0 and switch *auto* off. Use the same interface, buffer size and sample rate
   you record with.
2. Open a new session with a click track (Options › Click) and record-enable an audio track whose
   input is your microphone. Put on the headphones you record with.
3. Record about 8 bars. Clap sharply at the same time as each click (or, with a loudspeaker
   playing the click, let the microphone hear the click directly).
4. Stop, zoom into the first clap in the waveform until you can see single samples, and read how
   far the clap's attack lies *after* the click. Measure it with the selection length (in samples)
   from the click's position to the start of the clap.
5. Enter that number as the manual value in samples (or convert: ms = samples ÷ rate × 1000).
6. Record again. The claps should now start on the clicks. Repeat the measurement and correct the
   value if it is still off. Several claps give an average.

The right value changes with the buffer size and sample rate, and with a different interface or
driver mode: repeat the test after changing them.

## Limits

- Not verified on real hardware yet (the code is proven by tests with simulated takes only).
- The live waveform shown while recording is not shifted; only the clip placed at the end is.
- A negative offset (placing takes later) is not supported.
- The offset does not change monitoring (what you hear while playing), only where the take lands.
