"""Transcribe a WAV file with a given VoxType ASR preset (dev smoke test)."""

from __future__ import annotations

import argparse
import struct
import sys
import wave
from pathlib import Path

from voxtype_runtime.download_model import ensure_asr_model, resolve_preset, target_dir
from voxtype_runtime.recognizer.sherpa_onnx import try_create_sherpa_recognizer


def wav_to_pcm_s16le(path: Path) -> tuple[bytes, int]:
    with wave.open(str(path), "rb") as handle:
        channels = handle.getnchannels()
        sample_width = handle.getsampwidth()
        sample_rate = handle.getframerate()
        frames = handle.readframes(handle.getnframes())
    if sample_width != 2:
        raise ValueError(f"Expected 16-bit PCM, got sample width {sample_width}")
    if channels == 1:
        pcm = frames
    else:
        count = len(frames) // (sample_width * channels)
        samples = struct.unpack(f"<{count * channels}h", frames)
        pcm = struct.pack(f"<{count}h", *[samples[i] for i in range(0, len(samples), channels)])
    return pcm, sample_rate


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(description="Transcribe one WAV with VoxType runtime")
    parser.add_argument("--preset", default="fun_asr_nano")
    parser.add_argument("--wav", type=Path, required=True)
    parser.add_argument("--root", type=Path, default=None)
    parser.add_argument("--download", action="store_true", help="Download model if missing")
    parser.add_argument("--provider", default="cpu")
    args = parser.parse_args(argv)

    preset = resolve_preset(args.preset)
    model_dir = target_dir(args.root, preset)
    if args.download and not model_dir.is_dir():
        model_dir = ensure_asr_model(args.root, preset=preset)

    recognizer = try_create_sherpa_recognizer(
        model_dir,
        preset,
        provider=args.provider,
        num_threads=2,
    )
    if recognizer is None:
        print(f"Failed to load recognizer from {model_dir}", file=sys.stderr)
        raise SystemExit(1)

    pcm, sample_rate = wav_to_pcm_s16le(args.wav)
    text = recognizer.transcribe(pcm, sample_rate=sample_rate, language="zh")
    print(f"preset={preset}")
    print(f"model_dir={model_dir}")
    print(f"wav={args.wav}")
    print(f"text={text}")


if __name__ == "__main__":
    main()
