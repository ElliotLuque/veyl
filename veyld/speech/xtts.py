"""La voz de Veyl con XTTS-v2: un servidor que veyld deja cargado en la GPU.

Lee órdenes en líneas JSON por la entrada estándar y contesta en líneas JSON
por la salida:

    {"text": "Son las 16:20", "out": "/run/…/habla.wav"}
      → {"ok": true, "out": "/run/…/habla.wav", "ms": 412}

Al arrancar carga el modelo (de `--model`, una carpeta con config.json y
model.pth), lo calienta con una frase y dice {"ready": true}. Cada frase sale
con la voz de `--speaker`, a `--speed` y, con `--sox`, con el tratamiento de
robot de película: una capa una octava más grave casi inaudible, compresión y
ecualización de voz de cine, chorus y una sala metálica corta.
"""

import argparse
import json
import os
import subprocess
import sys
import tempfile
import time
import wave
import warnings

warnings.filterwarnings("ignore")


def say(message):
    sys.stdout.write(json.dumps(message) + "\n")
    sys.stdout.flush()


def write_wav(path, samples, rate):
    import numpy as np

    pcm = (np.clip(samples, -1.0, 1.0) * 32767).astype("<i2").tobytes()
    with wave.open(path, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(rate)
        w.writeframes(pcm)


def cinema(sox, raw, out):
    """El robot de película: la cadena de sox que se eligió oyéndola («cine suave»)."""
    with tempfile.TemporaryDirectory() as d:
        voice, low, mix = (os.path.join(d, n) for n in ("v.wav", "low.wav", "mix.wav"))
        run = lambda *args: subprocess.run([sox, *args], check=True, capture_output=True)
        run(raw, "-r", "48000", "-c", "1", voice)
        run(voice, low, "pitch", "-1200", "vol", "0.35")
        run("-m", voice, low, mix)
        run(
            mix, out,
            "highpass", "80",
            "compand", "0.005,0.12", "-60,-60,-30,-16,-10,-8,0,-5", "-3",
            "equalizer", "250", "1.2q", "-2",
            "equalizer", "2800", "1q", "2.5",
            "treble", "1.5", "7000",
            "chorus", "0.75", "0.85", "25", "0.25", "0.3", "2", "-s",
            "reverb", "12", "50", "8", "100", "5",
            "norm", "-3",
        )


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--model", required=True)
    parser.add_argument("--speaker", default="Damien Black")
    parser.add_argument("--language", default="es")
    parser.add_argument("--speed", type=float, default=1.2)
    parser.add_argument("--sox", default="")
    args = parser.parse_args()

    import torch
    from TTS.tts.configs.xtts_config import XttsConfig
    from TTS.tts.models.xtts import Xtts

    config = XttsConfig()
    config.load_json(os.path.join(args.model, "config.json"))
    model = Xtts.init_from_config(config)
    model.load_checkpoint(config, checkpoint_dir=args.model, use_deepspeed=False)
    gpu = torch.cuda.is_available()
    if gpu:
        model.cuda()
    speakers = model.speaker_manager.speakers
    if args.speaker not in speakers:
        say({"ready": False, "error": f"no hay voz «{args.speaker}»"})
        return 1
    latent, embedding = (speakers[args.speaker][k] for k in ("gpt_cond_latent", "speaker_embedding"))
    if gpu:
        latent, embedding = latent.cuda(), embedding.cuda()
    rate = config.audio.output_sample_rate

    def synth(text):
        result = model.inference(text, args.language, latent, embedding, speed=args.speed, temperature=0.65, enable_text_splitting=True)
        return result["wav"]

    synth("Hola.")  # Calienta: la primera frase en la GPU es lenta.
    say({"ready": True, "gpu": gpu, "rate": rate})

    for line in sys.stdin:
        try:
            order = json.loads(line)
            started = time.time()
            out = order["out"]
            wav = synth(order["text"])
            raw = out + ".crudo.wav"
            write_wav(raw, wav, rate)
            if args.sox:
                cinema(args.sox, raw, out)
                os.remove(raw)
            else:
                os.replace(raw, out)
            say({"ok": True, "out": out, "ms": int((time.time() - started) * 1000)})
        except Exception as e:  # noqa: BLE001 — cualquier fallo se cuenta y se sigue
            say({"ok": False, "error": str(e)})
    return 0


if __name__ == "__main__":
    sys.exit(main())
